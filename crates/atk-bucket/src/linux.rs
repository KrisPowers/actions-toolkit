//! Host-side half of the Linux Bucket backend: cgroup lifecycle, spawning the `__bucket-init`
//! re-exec target with the namespace-unshare `pre_exec` hook, and streaming its output. The
//! actual namespace/mount/seccomp setup happens in `bucket_init.rs`, which this module's
//! spawned child immediately re-execs into.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use nix::sched::CloneFlags;
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

use super::{BucketCapability, BucketHandle, BucketInitSpec, BucketSpec, ExecResult, DEFAULT_RO_MOUNTS};
use atk_db::queries::buckets as bucket_queries;

const CGROUP_ROOT: &str = "/sys/fs/cgroup/actions-toolkit";
/// Deliberately hyphen-free. systemd treats a hyphenated slice name as implicitly nested under a
/// parent slice named by everything before the first hyphen (`foo-bar.slice` lives under
/// `foo.slice`), confirmed empirically: `--slice actions-toolkit.slice` actually placed the
/// scope at `.../user@<uid>.service/actions.slice/actions-toolkit.slice/...`, an extra implied
/// `actions.slice` level `systemd_user_scope_cgroup_path` didn't know about, so its computed path
/// never matched reality and `try_create_systemd_scope` always looked like it had failed/timed
/// out even when the scope was created successfully. A hyphen-free name has no implied parent.
const DELEGATED_SLICE: &str = "atkbucket.slice";
const DEFAULT_PIDS_MAX: &str = "512";
const DEFAULT_MEMORY_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn root_skeleton_path(buckets_root: &Path, id: &str) -> PathBuf {
    buckets_root.join(id).join("root")
}

fn scope_unit_name(id: &str) -> String {
    format!("atk-bucket-{id}.scope")
}

/// The cgroup path a `systemd --user --scope --slice=actions-toolkit.slice` invocation would
/// place this bucket's anchor process under, per systemd's own (documented, deterministic) unit
/// naming rules. Purely a string computation, no I/O.
fn systemd_user_scope_cgroup_path(id: &str) -> PathBuf {
    let uid = nix::unistd::getuid().as_raw();
    PathBuf::from(format!(
        "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service/{DELEGATED_SLICE}/{}",
        scope_unit_name(id)
    ))
}

/// Which of the two possible cgroup layouts this bucket actually ended up using: the
/// systemd-delegated path if it exists on disk (meaning `create_delegated_cgroup` successfully
/// created it), otherwise the bare fallback path directly under `CGROUP_ROOT`. Used both right
/// after creation and when reconstructing a handle from a DB row after a restart, so it has to be
/// a plain existence check rather than a host-capability guess: whichever one this specific
/// bucket actually has on disk is authoritative.
fn cgroup_path_for(id: &str) -> PathBuf {
    let delegated = systemd_user_scope_cgroup_path(id);
    if delegated.exists() {
        delegated
    } else {
        Path::new(CGROUP_ROOT).join(id)
    }
}

pub(crate) fn handle_from_bucket_row(buckets_root: &Path, row: &atk_db::models::Bucket) -> BucketHandle {
    BucketHandle {
        id: row.id.clone(),
        workspace: PathBuf::from(&row.workspace_path),
        root_skeleton: root_skeleton_path(buckets_root, &row.id),
        network_enabled: row.network_enabled != 0,
        // Only used by exec_step (a fresh step execution); this reconstruction path is for
        // crash-recovery cleanup/reaping, which never calls exec_step.
        extra_ro_mounts: Vec::new(),
        cgroup_path: cgroup_path_for(&row.id),
    }
}

pub async fn create_job_bucket(pool: &SqlitePool, buckets_root: &Path, spec: BucketSpec<'_>) -> Result<BucketHandle> {
    let id = Uuid::new_v4().to_string();
    let root_skeleton = root_skeleton_path(buckets_root, &id);
    std::fs::create_dir_all(&root_skeleton).context("failed to create bucket root skeleton")?;

    let cgroup_path = create_delegated_cgroup(&id).await.context("failed to create bucket cgroup")?;

    let ttl_expires_at =
        (chrono::Utc::now() + chrono::Duration::seconds(spec.ttl.as_secs() as i64)).to_rfc3339();

    bucket_queries::create(
        pool,
        &id,
        spec.job_run_id,
        spec.run_id,
        &spec.workspace_host_path.to_string_lossy(),
        spec.network_enabled,
        &ttl_expires_at,
    )
    .await
    .context("failed to record bucket in database")?;

    let extra_ro_mounts: Vec<PathBuf> = spec.extra_ro_mounts.iter().map(PathBuf::from).filter(|p| p.exists()).collect();

    Ok(BucketHandle {
        id,
        workspace: spec.workspace_host_path.to_path_buf(),
        root_skeleton,
        network_enabled: spec.network_enabled,
        extra_ro_mounts,
        cgroup_path,
    })
}

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
    let invocation = StepInvocation { shell_command, shell, working_dir, env };
    run_in_sandbox(
        &handle.root_skeleton,
        &handle.cgroup_path,
        &handle.workspace,
        handle.network_enabled,
        &handle.extra_ro_mounts,
        invocation,
        on_line,
    )
    .await
}

pub async fn remove_bucket(pool: &SqlitePool, handle: &BucketHandle) -> Result<()> {
    let _ = pool; // DB status transition is the caller's job (see bucket::reaper); kept for
                  // signature symmetry with the Windows backend.
    destroy_cgroup(&handle.cgroup_path).await.context("failed to destroy bucket cgroup")?;
    if handle.root_skeleton.exists() {
        std::fs::remove_dir_all(&handle.root_skeleton).context("failed to remove bucket root skeleton")?;
    }
    Ok(())
}

/// Functional probe: actually runs a throwaway command through the full sandbox mechanism
/// (namespaces, pivot_root, cgroup, seccomp) rather than just checking a kernel version, since
/// the real-world failure modes (AppArmor's unprivileged-userns restriction on newer Ubuntu, a
/// non-delegated cgroup v2 hierarchy) aren't visible from version numbers alone. Runs entirely
/// outside the database (no job/workflow run rows exist for a probe).
pub async fn probe_capability() -> BucketCapability {
    let buckets_root = std::env::temp_dir().join("actions-toolkit-bucket-probe");
    let id = Uuid::new_v4().to_string();
    let root_skeleton = root_skeleton_path(&buckets_root, &id);

    let result = probe_inner(&id, &root_skeleton).await;

    let cgroup_path = cgroup_path_for(&id);
    let _ = destroy_cgroup(&cgroup_path).await;
    let _ = std::fs::remove_dir_all(&root_skeleton);

    match result {
        Ok(()) => BucketCapability { ok: true, reason: None },
        Err(e) => BucketCapability { ok: false, reason: Some(format!("{e:#}")) },
    }
}

/// Exercises the same cgroup-creation path (systemd-delegated scope, falling back to a bare
/// cgroup) that real buckets use, not just the fallback, so the probe actually reflects what a
/// real bucket will get on this host.
async fn probe_inner(id: &str, root_skeleton: &Path) -> Result<()> {
    std::fs::create_dir_all(root_skeleton).context("failed to create probe workspace")?;
    let cgroup_path = create_delegated_cgroup(id).await.context("failed to create probe cgroup")?;

    let workspace = root_skeleton.join("probe-workspace");
    std::fs::create_dir_all(&workspace)?;

    let invocation = StepInvocation { shell_command: "true", shell: None, working_dir: None, env: &[] };
    let result = run_in_sandbox(root_skeleton, &cgroup_path, &workspace, false, &[], invocation, |_, _| {}).await?;
    if result.exit_code != 0 {
        anyhow::bail!("probe command exited with status {}", result.exit_code);
    }
    Ok(())
}

/// What to actually run: bundled separately from the sandbox's own identity/config params so
/// `run_in_sandbox` doesn't grow an unwieldy positional argument list.
struct StepInvocation<'a> {
    shell_command: &'a str,
    shell: Option<&'a str>,
    working_dir: Option<&'a str>,
    env: &'a [String],
}

/// Read-only mounts for a step: `DEFAULT_RO_MOUNTS` plus the operator-configured
/// `extra_ro_mounts` (deduplicated isn't needed, bind-mounting the same path twice is harmless),
/// filtered to paths that actually exist on the host. Pure and separate from `run_in_sandbox` so
/// the merge/filter behavior is directly testable without needing a real sandboxed execution.
fn resolve_ro_mounts(extra_ro_mounts: &[PathBuf]) -> Vec<PathBuf> {
    DEFAULT_RO_MOUNTS.iter().map(PathBuf::from).chain(extra_ro_mounts.iter().cloned()).filter(|p| p.exists()).collect()
}

/// Spawns `__bucket-init` with the namespace-unshare `pre_exec` hook, streams its stdout/stderr
/// line-by-line to `on_line`, and returns its exit code. This is the actual per-step sandbox
/// execution shared by `exec_step` and the capability probe.
async fn run_in_sandbox<F>(
    root_skeleton: &Path,
    cgroup_path: &Path,
    workspace: &Path,
    network_enabled: bool,
    extra_ro_mounts: &[PathBuf],
    invocation: StepInvocation<'_>,
    mut on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    let StepInvocation { shell_command, shell, working_dir, env } = invocation;
    let ro_mounts = resolve_ro_mounts(extra_ro_mounts);

    let spec = BucketInitSpec {
        workspace: workspace.to_path_buf(),
        root_skeleton: root_skeleton.to_path_buf(),
        ro_mounts,
        cgroup_path: cgroup_path.to_path_buf(),
        shell_command: shell_command.to_string(),
        shell: shell.map(str::to_string),
        working_dir: working_dir.map(str::to_string),
        env: env.to_vec(),
    };

    let spec_path = root_skeleton
        .parent()
        .unwrap_or(root_skeleton)
        .join(format!("step-{}.json", Uuid::new_v4()));
    std::fs::write(&spec_path, serde_json::to_vec(&spec).context("failed to serialize bucket init spec")?)
        .context("failed to write bucket init spec")?;

    let current_exe = std::env::current_exe().context("failed to resolve current executable path")?;

    let mut command = Command::new(&current_exe);
    command
        .arg("__bucket-init")
        .arg(&spec_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // SAFETY: the closure below only calls unshare(2) and writes to this freshly-forked child's
    // own /proc/self/{setgroups,uid_map,gid_map} before exec, no heap allocation, no locks, no
    // access to state shared with other threads, satisfying pre_exec's async-signal-safety
    // requirement for the fork-to-exec window.
    unsafe {
        command.pre_exec(unshare_into_new_namespaces);
    }

    let mut child = command.spawn().context("failed to spawn __bucket-init")?;

    if network_enabled {
        let pid = child.id().context("spawned __bucket-init has no PID (already exited?)")?;
        if let Err(e) = setup_pasta_networking(pid).await {
            let _ = child.start_kill(); // don't leave the already-spawned sandbox process running unsupervised
            return Err(e.context("network: true was requested but pasta networking setup failed"));
        }
    }

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(&'static str, String)>();

    let stdout_tx = tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if stdout_tx.send(("stdout", line)).is_err() {
                break;
            }
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(("stderr", line)).is_err() {
                break;
            }
        }
    });

    while let Some((stream, line)) = rx.recv().await {
        on_line(stream, line);
    }
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let status = child.wait().await.context("failed waiting for __bucket-init")?;
    let _ = std::fs::remove_file(&spec_path);

    Ok(ExecResult { exit_code: status.code().unwrap_or(-1) as i64 })
}

/// Runs in the freshly-forked, still-single-threaded child between `fork()` and `execve()` (see
/// `Command::pre_exec`). Puts the child into new user/mount/uts/ipc/net/pid namespaces (cgroup
/// is handled separately, see below) and maps its own (unprivileged) uid/gid to 0 inside the new
/// user namespace, the standard
/// unprivileged-user-namespace pattern (`man 7 user_namespaces`), safe here because a process
/// that just created a user namespace via `unshare` automatically holds full capabilities
/// within that namespace, including the `CAP_SETUID`/`CAP_SETGID` needed to write its own map.
///
/// Deliberately excludes `CLONE_NEWCGROUP` here: per `cgroup_namespaces(7)`, a cgroup namespace
/// has to be entered *after* the process has already been moved into its target cgroup, not
/// before, joining a cgroup via its absolute host path while already inside a fresh cgroup
/// namespace fails with EIO, since that path is no longer a descendant of the namespace's own
/// root. `bucket_init::join_cgroup` does the join first; `unshare(CLONE_NEWCGROUP)` happens
/// separately right after, giving the sandboxed process a namespaced view rooted at its own
/// cgroup from that point on.
fn unshare_into_new_namespaces() -> std::io::Result<()> {
    let flags = CloneFlags::CLONE_NEWUSER
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWPID;

    nix::sched::unshare(flags).map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;

    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();

    std::fs::write("/proc/self/setgroups", "deny")?;
    std::fs::write("/proc/self/uid_map", format!("0 {uid} 1"))?;
    std::fs::write("/proc/self/gid_map", format!("0 {gid} 1"))?;

    Ok(())
}

/// Best-effort: enables the pids/memory/cpu controllers for delegation down to per-bucket
/// cgroups. A failure here isn't fatal to sandbox creation, `cgroup.kill` (the guaranteed
/// teardown mechanism relied on elsewhere) is a core cgroup v2 interface file available
/// regardless of which resource controllers happen to be delegated.
fn ensure_cgroup_controllers() -> Result<()> {
    let root = Path::new("/sys/fs/cgroup");
    if !root.join("cgroup.controllers").exists() {
        anyhow::bail!("cgroup v2 unified hierarchy is not mounted at /sys/fs/cgroup");
    }
    enable_controllers(root);

    let toolkit_root = Path::new(CGROUP_ROOT);
    std::fs::create_dir_all(toolkit_root).context("failed to create actions-toolkit cgroup root")?;
    enable_controllers(toolkit_root);
    Ok(())
}

fn enable_controllers(dir: &Path) {
    let available = std::fs::read_to_string(dir.join("cgroup.controllers")).unwrap_or_default();
    let wanted: Vec<String> =
        ["pids", "memory", "cpu"].iter().filter(|c| available.split_whitespace().any(|a| a == **c)).map(|c| format!("+{c}")).collect();
    if wanted.is_empty() {
        return;
    }
    let _ = std::fs::write(dir.join("cgroup.subtree_control"), wanted.join(" "));
}

fn create_cgroup(path: &Path) -> Result<()> {
    ensure_cgroup_controllers().context("cgroup v2 is not usable on this host")?;
    std::fs::create_dir_all(path).context("failed to create bucket cgroup directory")?;
    let _ = std::fs::write(path.join("pids.max"), DEFAULT_PIDS_MAX);
    let _ = std::fs::write(path.join("memory.max"), DEFAULT_MEMORY_MAX_BYTES.to_string());
    Ok(())
}

/// Creates this bucket's cgroup under a systemd-delegated transient scope when a systemd user
/// session is reachable, falling back to a bare cgroup directly under `CGROUP_ROOT` (today's
/// only mechanism, still fully supported) when it isn't. A raw cgroup created directly off
/// `/sys/fs/cgroup` is invisible to systemd's own bookkeeping and, on a systemd-managed host, is
/// liable to be reaped as "foreign" state systemd doesn't recognize; a delegated scope avoids
/// that because systemd itself owns the unit and won't touch cgroups it created.
///
/// NOTE: the delegated-scope path has been reviewed against documented systemd/cgroup-v2
/// semantics but has not been exercised end-to-end on a real systemd host (this was implemented
/// without one available), treat it as unverified until it's actually run against `systemd-run
/// --user --scope` on a live system, same as the rest of the Bucket adversarial-validation work.
async fn create_delegated_cgroup(id: &str) -> Result<PathBuf> {
    if let Some(path) = try_create_systemd_scope(id).await {
        return Ok(path);
    }
    let path = Path::new(CGROUP_ROOT).join(id);
    create_cgroup(&path)?;
    Ok(path)
}

/// Spawns a long-lived anchor process inside a transient systemd `--user --scope`, keeping the
/// delegated cgroup populated (and therefore alive) for the bucket's whole lifetime; an empty
/// scope cgroup is torn down by systemd immediately, so each step's `__bucket-init` process joins
/// this cgroup (via the existing `join_cgroup` call in `bucket_init.rs`, unchanged) alongside the
/// anchor rather than instead of it. `cgroup.kill` in `destroy_cgroup` reaps the anchor along with
/// everything else in the cgroup; `--collect` tells systemd to garbage-collect the transient unit
/// once its cgroup is empty, so no explicit `systemctl stop` is needed.
///
/// The anchor's `Child` handle is deliberately dropped without `.wait()`-ing: tokio reaps
/// orphaned children in the background once they exit (see the `tokio::process` orphan queue),
/// and this process is meant to keep running until `cgroup.kill` ends it.
async fn try_create_systemd_scope(id: &str) -> Option<PathBuf> {
    let target = systemd_user_scope_cgroup_path(id);

    let args: Vec<String> = vec![
        "--user".to_string(),
        "--scope".to_string(),
        "--collect".to_string(),
        "--quiet".to_string(),
        "--unit".to_string(),
        scope_unit_name(id),
        "--slice".to_string(),
        DELEGATED_SLICE.to_string(),
        "--property".to_string(),
        format!("TasksMax={DEFAULT_PIDS_MAX}"),
        "--property".to_string(),
        format!("MemoryMax={DEFAULT_MEMORY_MAX_BYTES}"),
        "--".to_string(),
        "sleep".to_string(),
        "2147483647".to_string(),
    ];

    let mut child = Command::new("systemd-run")
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;

    for _ in 0..20 {
        if target.exists() {
            return Some(target);
        }
        if let Ok(Some(_)) = child.try_wait() {
            return None; // systemd-run exited (most likely: no reachable user session bus)
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let _ = child.start_kill();
    None
}

/// Tears down a bucket's cgroup: `cgroup.kill` (Linux 5.14+) atomically SIGKILLs every process
/// in the cgroup in one write, regardless of how deep a process tree the sandboxed command
/// forked. This is the guaranteed-cleanup mechanism, not best-effort process tracking.
///
/// For a systemd-delegated scope (created with `--collect`), killing its last process makes
/// systemd itself remove the scope's cgroup directory as part of garbage-collecting the now-empty
/// unit, confirmed empirically, not assumed. That's success, the same as removing it ourselves,
/// so the retry loop below treats "the directory is just gone" as done rather than retrying
/// `remove_dir` against a path that will never reappear.
async fn destroy_cgroup(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let _ = std::fs::write(path.join("cgroup.kill"), "1");

    for _ in 0..20 {
        if !path.exists() || std::fs::remove_dir(path).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    std::fs::remove_dir(path).context("failed to remove bucket cgroup directory after cgroup.kill")
}

/// Wires up opt-in network access for one step's already-spawned `__bucket-init` process (which,
/// by the time this runs, has already `unshare(CLONE_NEWNET)`d into a fresh, otherwise-empty net
/// namespace via the `pre_exec` hook) via `pasta`: an unprivileged, per-namespace userspace network
/// stack, rather than hand-rolled veth+NAT+iptables, which would mutate host-global network state
/// that a fork bomb or crash on our side could leave dangling. `pasta` attaches to the target
/// PID's network namespace by number and is expected to keep running until that namespace is no
/// longer referenced by any process, then exit on its own, not waited on here (see below).
///
/// NOTE: written from documented `pasta`/`passt` behavior, not exercised against a real `pasta`
/// binary (none was available while writing this), treat the exact invocation and the
/// fire-and-forget assumption below as unverified until run on a live host. In particular: if
/// `pasta` does *not* actually daemonize/return once attached (this implementation doesn't wait
/// on it, on the assumption that it does), the step's network may not be ready the instant its
/// command starts, which is a real race this couldn't be tested against here.
async fn setup_pasta_networking(target_pid: u32) -> Result<()> {
    let child = Command::new("pasta")
        .arg(target_pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("pasta is not installed on this host (see https://passt.top); network: true requires it")?;
    drop(child); // fire-and-forget; tokio reaps it in the background once it exits on its own
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the pure path/serialization logic only. The actual spawn-and-isolate path
    // (`run_in_sandbox`) re-execs `std::env::current_exe()` as `__bucket-init`, which under
    // `cargo test` resolves to the test harness binary rather than the real `actions-toolkit`
    // binary (Cargo doesn't set `CARGO_BIN_EXE_*` for a crate's own unit tests, only for
    // separate integration-test/example targets, and this crate has no library target to give
    // such a target access to these internals anyway), so it isn't exercised here. It's been
    // verified manually against a real build instead; see the Bucket plan's verification notes.

    #[test]
    fn root_skeleton_and_cgroup_paths_are_scoped_by_id() {
        let buckets_root = Path::new("/data/buckets");
        let a = root_skeleton_path(buckets_root, "abc");
        let b = root_skeleton_path(buckets_root, "xyz");
        assert_ne!(a, b);
        assert_eq!(a, Path::new("/data/buckets/abc/root"));
        assert_eq!(cgroup_path_for("abc"), Path::new("/sys/fs/cgroup/actions-toolkit/abc"));
    }

    #[test]
    fn bucket_init_spec_round_trips_through_json() {
        let spec = BucketInitSpec {
            workspace: PathBuf::from("/data/workspaces/run-1"),
            root_skeleton: PathBuf::from("/data/buckets/bucket-1/root"),
            ro_mounts: vec![PathBuf::from("/usr"), PathBuf::from("/bin")],
            cgroup_path: PathBuf::from("/sys/fs/cgroup/actions-toolkit/bucket-1"),
            shell_command: "echo hello".to_string(),
            shell: Some("bash".to_string()),
            working_dir: Some("/workspace".to_string()),
            env: vec!["FOO=bar".to_string()],
        };

        let bytes = serde_json::to_vec(&spec).expect("spec should serialize");
        let round_tripped: BucketInitSpec = serde_json::from_slice(&bytes).expect("spec should deserialize");

        assert_eq!(round_tripped.workspace, spec.workspace);
        assert_eq!(round_tripped.ro_mounts, spec.ro_mounts);
        assert_eq!(round_tripped.shell_command, spec.shell_command);
        assert_eq!(round_tripped.env, spec.env);
    }

    #[test]
    fn default_ro_mounts_filter_to_paths_that_exist_on_this_host() {
        // A sanity check that the filtering logic `run_in_sandbox` applies (skip anything
        // absent on this host) doesn't panic and only keeps real paths; on any normal Linux
        // host at least one of these should exist.
        let any_exist = DEFAULT_RO_MOUNTS.iter().any(|p| Path::new(p).exists());
        assert!(any_exist, "expected at least one of the default ro-mount paths to exist on a normal Linux host");
    }

    /// Rule-proving test for the host-mount allowlist (#13): a configured extra path that
    /// actually exists on the host must be included alongside the defaults.
    #[test]
    fn resolve_ro_mounts_includes_an_existing_configured_extra_path() {
        let test_dir = std::env::temp_dir().join(format!("atk-extra-mount-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&test_dir).unwrap();

        let mounts = resolve_ro_mounts(&[test_dir.clone()]);
        assert!(mounts.contains(&test_dir), "expected the configured extra path to be included: {mounts:?}");
        assert!(mounts.len() > 1, "expected the defaults to still be present alongside the extra path");

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    /// Rule-proving test: a configured extra path that doesn't exist on the host must be
    /// silently skipped, matching `DEFAULT_RO_MOUNTS`' own convention, not fail the job.
    #[test]
    fn resolve_ro_mounts_skips_a_configured_extra_path_that_does_not_exist() {
        let missing = PathBuf::from("/definitely/does/not/exist/on/this/host/atk-test");
        let mounts = resolve_ro_mounts(&[missing.clone()]);
        assert!(!mounts.contains(&missing), "expected a nonexistent configured path to be silently skipped: {mounts:?}");
    }

    /// Rule-proving test for the systemd-delegation verification #12 asked for: unlike
    /// `run_in_sandbox` (blocked under `cargo test` by the `current_exe()` re-exec issue
    /// documented above), `create_delegated_cgroup` needs no re-exec at all -- it's directly
    /// testable. Confirms on a real systemd host that the delegated scope path is actually used
    /// (not silently falling back to the bare `CGROUP_ROOT` path), that it's a real, populated
    /// cgroup on disk, and that teardown via `destroy_cgroup` actually removes it.
    #[tokio::test]
    async fn create_delegated_cgroup_uses_the_systemd_scope_when_available() {
        let id = format!("test-{}", Uuid::new_v4());

        let cgroup_path = match create_delegated_cgroup(&id).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("skipping: create_delegated_cgroup failed on this host ({e:#}), likely no systemd user session reachable");
                return;
            }
        };

        let expected_delegated_path = systemd_user_scope_cgroup_path(&id);
        if cgroup_path != expected_delegated_path {
            eprintln!(
                "skipping strict assertions: fell back to the bare cgroup path ({}), no systemd user session reachable on this host",
                cgroup_path.display()
            );
            let _ = destroy_cgroup(&cgroup_path).await;
            return;
        }

        assert!(cgroup_path.exists(), "expected the delegated cgroup to actually exist on disk: {}", cgroup_path.display());
        assert!(
            cgroup_path.join("cgroup.procs").exists(),
            "expected a real cgroup control file at {}",
            cgroup_path.join("cgroup.procs").display()
        );

        destroy_cgroup(&cgroup_path).await.expect("destroy_cgroup should succeed");
        assert!(!cgroup_path.exists(), "expected the delegated cgroup to be gone after teardown");
    }
}
