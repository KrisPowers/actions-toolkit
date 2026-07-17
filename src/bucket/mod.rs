//! Bucket: a native, per-step sandbox used to run workflow steps without Docker.
//!
//! Each `run:` step gets its own temporary, isolated execution environment (filesystem,
//! network, and process tree) via OS-native primitives (Linux namespaces/cgroups/seccomp,
//! Windows AppContainer/Job Objects), rather than a container runtime. The public surface here
//! mirrors `runner::docker`'s free-function shape (`create_job_bucket`/`exec_step`/
//! `remove_bucket`) so callers don't need to know which OS backend is active.

pub mod reaper;

#[cfg(target_os = "linux")]
pub mod bucket_init;
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
use sqlx::SqlitePool;

/// Default lifetime for a bucket if nothing else expires it first (mirrors GitHub Actions'
/// own default job timeout), used as the backstop the TTL reaper sweeps against.
pub const DEFAULT_TTL: Duration = Duration::from_secs(6 * 60 * 60);

/// Host directories bind-mounted read-only into every bucket so `run:` steps can still invoke
/// system-package-manager-installed toolchains (git, a system Python/Node, etc.) despite the
/// sandbox otherwise having no visibility into the host filesystem. Paths that don't exist on
/// the host are silently skipped. This is a conservative default, not a completeness guarantee;
/// toolchains installed under a user's home directory (nvm, pyenv, `~/.cargo`) are not covered
/// and need an explicit host-mount allowlist entry (settings-level, not yet wired — see plan).
pub const DEFAULT_RO_MOUNTS: &[&str] =
    &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc/ssl/certs", "/etc/alternatives", "/etc/resolv.conf"];

pub struct BucketSpec<'a> {
    pub workspace_host_path: &'a Path,
    pub run_id: &'a str,
    pub job_run_id: &'a str,
    pub network_enabled: bool,
    pub ttl: Duration,
}

/// A live handle to a job's sandbox scaffolding (its cgroup and root-skeleton directory). Each
/// step run inside it gets a fresh set of namespaces via `exec_step`, but shares this cgroup
/// (for resource limits and guaranteed teardown) and the same host workspace directory (which
/// is what actually carries state between steps, the same way it does for `docker.rs`'s
/// bind-mounted `/workspace`).
pub struct BucketHandle {
    pub id: String,
    pub workspace: PathBuf,
    pub(crate) root_skeleton: PathBuf,
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

/// Everything `__bucket-init` needs to set up one step's sandbox and run its command, handed
/// off via a spec file rather than CLI args/env to avoid shell-escaping a shell command through
/// another layer of argv.
#[derive(Debug, Serialize, Deserialize)]
pub struct BucketInitSpec {
    pub workspace: PathBuf,
    pub root_skeleton: PathBuf,
    pub ro_mounts: Vec<PathBuf>,
    pub cgroup_path: PathBuf,
    pub shell_command: String,
    pub shell: Option<String>,
    pub working_dir: Option<String>,
    pub env: Vec<String>,
}

/// Functional probe of whether this host can actually run buckets (not just an OS-version
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

pub async fn create_job_bucket(pool: &SqlitePool, buckets_root: &Path, spec: BucketSpec<'_>) -> Result<BucketHandle> {
    #[cfg(target_os = "linux")]
    {
        linux::create_job_bucket(pool, buckets_root, spec).await
    }
    #[cfg(target_os = "windows")]
    {
        windows::create_job_bucket(pool, buckets_root, spec).await
    }
}

/// Run one step's shell command in a fresh sandbox scoped to `handle`'s workspace/cgroup,
/// streaming each output line to `on_line(stream, message)` as it arrives (same shape as
/// `runner::docker::exec_step`, so callers can swap backends without changing their callback).
pub async fn exec_step<F>(
    handle: &BucketHandle,
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

pub async fn remove_bucket(pool: &SqlitePool, handle: &BucketHandle) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::remove_bucket(pool, handle).await
    }
    #[cfg(target_os = "windows")]
    {
        windows::remove_bucket(pool, handle).await
    }
}

/// Rebuilds a `BucketHandle` from a persisted DB row rather than a live `create_job_bucket`
/// call, used by the reaper (TTL sweep, startup crash reconciliation) where the process that
/// created the bucket may be long gone. The scaffolding paths are deterministic from
/// `buckets_root` + the bucket's own id, so no extra state needs to round-trip through the DB.
pub(crate) fn handle_from_bucket_row(buckets_root: &Path, row: &crate::db::models::Bucket) -> BucketHandle {
    #[cfg(target_os = "linux")]
    {
        linux::handle_from_bucket_row(buckets_root, row)
    }
    #[cfg(target_os = "windows")]
    {
        windows::handle_from_bucket_row(buckets_root, row)
    }
}
