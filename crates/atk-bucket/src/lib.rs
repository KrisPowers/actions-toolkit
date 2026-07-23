//! Shard: the native, per-job OS isolation a shell creates and tears down for each job in its
//! run, without Docker. A shard is a child of the shell that owns it, the same way a shell is a
//! child of its bucket.
//!
//! Each `run:` step gets its own temporary, isolated execution environment (filesystem,
//! network, and process tree) via OS-native primitives (Linux namespaces/cgroups/seccomp,
//! Windows AppContainer/Job Objects), rather than a container runtime. The public surface here
//! mirrors `runner::docker`'s free-function shape (`create_job_shard`/`exec_step`/
//! `remove_shard`) so callers don't need to know which OS backend is active.

pub mod reaper;

#[cfg(target_os = "linux")]
pub mod shard_init;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod seccomp_policy;
#[cfg(target_os = "windows")]
mod windows;

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Default lifetime for a bucket if nothing else expires it first (mirrors GitHub Actions'
/// own default job timeout), used as the backstop the TTL reaper sweeps against.
pub const DEFAULT_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Host directories bind-mounted read-only into every bucket so `run:` steps can still invoke
/// system-package-manager-installed toolchains (git, a system Python/Node, etc.) despite the
/// sandbox otherwise having no visibility into the host filesystem. Paths that don't exist on
/// the host are silently skipped. This is a conservative default, not a completeness guarantee;
/// toolchains installed under a user's home directory (nvm, pyenv, `~/.cargo`) are not covered
/// and need an explicit host-mount allowlist entry (settings-level, not yet wired, see plan).
pub const DEFAULT_RO_MOUNTS: &[&str] =
    &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc/ssl/certs", "/etc/alternatives", "/etc/resolv.conf"];

pub struct ShardSpec<'a> {
    pub workspace_host_path: &'a Path,
    pub network_enabled: bool,
    /// Not consumed by this crate (it does no database bookkeeping of its own, see
    /// `create_job_shard`'s doc comment) — kept here so a caller building a `ShardSpec`
    /// already has it at hand to also record alongside whatever row it writes for this sandbox.
    pub ttl: Duration,
    /// Additive, operator-configured host paths (`settings.bucket_host_mounts_json`) exposed
    /// read-only on top of `DEFAULT_RO_MOUNTS`, for toolchains installed under a user's home
    /// directory (nvm, pyenv, `~/.cargo`) that the conservative built-in defaults don't cover.
    /// Linux: bind-mounted read-only, same as the defaults. Windows: granted read+execute ACL for
    /// this bucket's AppContainer SID, same lifetime as the workspace grant. Paths that don't
    /// exist on the host are silently skipped, matching `DEFAULT_RO_MOUNTS`' own behavior.
    pub extra_ro_mounts: &'a [String],
}

/// A live handle to a job's sandbox scaffolding (its cgroup and root-skeleton directory). Each
/// step run inside it gets a fresh set of namespaces via `exec_step`, but shares this cgroup
/// (for resource limits and guaranteed teardown) and the same host workspace directory (which
/// is what actually carries state between steps, the same way it does for `docker.rs`'s
/// bind-mounted `/workspace`).
#[derive(Clone)]
pub struct ShardHandle {
    pub id: String,
    pub workspace: PathBuf,
    pub(crate) root_skeleton: PathBuf,
    pub(crate) network_enabled: bool,
    /// Filtered to paths that actually exist on the host at creation time, same convention as
    /// `DEFAULT_RO_MOUNTS`. Read by Linux's `exec_step` on every step (mounts don't persist
    /// across processes the way an ACL grant does); Windows only needs this at creation time to
    /// grant the ACL once, so it's write-only there, hence the cfg_attr below.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub(crate) extra_ro_mounts: Vec<PathBuf>,
    #[cfg(target_os = "linux")]
    pub(crate) cgroup_path: PathBuf,
}

pub struct ExecResult {
    pub exit_code: i64,
}

#[derive(Debug, Clone)]
pub struct BucketCapability {
    pub ok: bool,
    pub reason: Option<String>,
}

/// Everything `__shard-init` needs to set up one step's sandbox and run its command, handed
/// off via a spec file rather than CLI args/env to avoid shell-escaping a shell command through
/// another layer of argv.
#[derive(Debug, Serialize, Deserialize)]
pub struct ShardInitSpec {
    pub workspace: PathBuf,
    pub root_skeleton: PathBuf,
    pub ro_mounts: Vec<PathBuf>,
    pub cgroup_path: PathBuf,
    pub shell_command: String,
    pub shell: Option<String>,
    pub working_dir: Option<String>,
    pub env: Vec<String>,
}

/// Functional probe of whether this host can actually run shards (not just an OS-version
/// check): exercises the real create/exec/remove path end-to-end against a throwaway
/// command, so it also catches host-specific restrictions like the AppArmor
/// `unprivileged_userns_restriction` feature on newer Ubuntu, or a non-delegated cgroup v2
/// hierarchy.
pub async fn probe_capability() -> BucketCapability {
    #[cfg(target_os = "linux")]
    {
        linux::probe_capability().await
    }
    #[cfg(target_os = "windows")]
    {
        windows::probe_capability().await
    }
}

/// Purely OS-level: sets up the sandbox scaffolding and returns a handle to it. Does not touch
/// any database — the caller (a shell, via its `RunClient`) is responsible for recording whatever
/// bookkeeping row it needs, since this crate has no way to reach one from inside a shell process.
pub async fn create_job_shard(buckets_root: &Path, spec: ShardSpec<'_>) -> Result<ShardHandle> {
    #[cfg(target_os = "linux")]
    {
        linux::create_job_shard(buckets_root, spec).await
    }
    #[cfg(target_os = "windows")]
    {
        windows::create_job_shard(buckets_root, spec).await
    }
}

/// Run one step's shell command in a fresh sandbox scoped to `handle`'s workspace/cgroup,
/// streaming each output line to `on_line(stream, message)` as it arrives (same shape as
/// `runner::docker::exec_step`, so callers can swap backends without changing their callback).
pub async fn exec_step<F>(
    handle: &ShardHandle,
    shell_command: &str,
    shell: Option<&str>,
    working_dir: Option<&str>,
    env: &[String],
    on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    #[cfg(target_os = "linux")]
    {
        linux::exec_step(handle, shell_command, shell, working_dir, env, on_line).await
    }
    #[cfg(target_os = "windows")]
    {
        windows::exec_step(handle, shell_command, shell, working_dir, env, on_line).await
    }
}

/// Raw accounting counters for one shard, read fresh each call (no rate computed here — the
/// caller, which controls the sampling interval, turns `cpu_usage_usec` deltas into a percentage).
/// Every field is `None` when the host has no way to read it: on Windows, `ShardHandle` carries no
/// persisted Job Object handle (one is created fresh per-step inside `windows::run_step_blocking`
/// and dropped immediately after), so this always returns all-`None` there today. A caller getting
/// an all-`None` result should fall back to attributing that shard's activity to its parent shell's
/// own process-tree sample instead of showing a broken/zeroed card.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShardAccounting {
    pub memory_bytes: Option<u64>,
    pub cpu_usage_usec: Option<u64>,
    pub process_count: Option<u64>,
}

pub fn read_shard_accounting(handle: &ShardHandle) -> ShardAccounting {
    #[cfg(target_os = "linux")]
    {
        linux::read_shard_accounting(handle)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = handle;
        ShardAccounting::default()
    }
}

pub async fn remove_shard(handle: &ShardHandle) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::remove_shard(handle).await
    }
    #[cfg(target_os = "windows")]
    {
        windows::remove_shard(handle).await
    }
}

/// Rebuilds a `ShardHandle` from a persisted DB row rather than a live `create_job_shard`
/// call, used by the reaper (TTL sweep, startup crash reconciliation) where the shell that
/// created the shard may be long gone. The scaffolding paths are deterministic from
/// `buckets_root` + the shard's own id, so no extra state needs to round-trip through the DB.
pub fn handle_from_shard_row(buckets_root: &Path, row: &atk_db::models::Shard) -> ShardHandle {
    #[cfg(target_os = "linux")]
    {
        linux::handle_from_shard_row(buckets_root, row)
    }
    #[cfg(target_os = "windows")]
    {
        windows::handle_from_shard_row(buckets_root, row)
    }
}
