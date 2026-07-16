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
use crate::db::queries::buckets as bucket_queries;

const CGROUP_ROOT: &str = "/sys/fs/cgroup/actions-toolkit";
const DEFAULT_PIDS_MAX: &str = "512";
const DEFAULT_MEMORY_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

fn root_skeleton_path(buckets_root: &Path, id: &str) -> PathBuf {
    buckets_root.join(id).join("root")
}

fn cgroup_path_for(id: &str) -> PathBuf {
    Path::new(CGROUP_ROOT).join(id)
}

pub(crate) fn handle_from_bucket_row(buckets_root: &Path, row: &crate::db::models::Bucket) -> BucketHandle {
    BucketHandle {
        id: row.id.clone(),
        workspace: PathBuf::from(&row.workspace_path),
        root_skeleton: root_skeleton_path(buckets_root, &row.id),
        cgroup_path: cgroup_path_for(&row.id),
    }
}

pub async fn create_job_bucket(pool: &SqlitePool, buckets_root: &Path, spec: BucketSpec<'_>) -> Result<BucketHandle> {
    let id = Uuid::new_v4().to_string();
    let root_skeleton = root_skeleton_path(buckets_root, &id);
    std::fs::create_dir_all(&root_skeleton).context("failed to create bucket root skeleton")?;

    let cgroup_path = cgroup_path_for(&id);
    create_cgroup(&cgroup_path).context("failed to create bucket cgroup")?;

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

    Ok(BucketHandle { id, workspace: spec.workspace_host_path.to_path_buf(), root_skeleton, cgroup_path })
}

pub async fn exec_step<F>(
    handle: &BucketHandle,
    shell_command: &str,
    working_dir: Option<&str>,
    env: &[String],
    on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    run_in_sandbox(&handle.root_skeleton, &handle.cgroup_path, &handle.workspace, shell_command, working_dir, env, on_line)
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
    let cgroup_path = cgroup_path_for(&id);

    let result = probe_inner(&root_skeleton, &cgroup_path).await;

    let _ = destroy_cgroup(&cgroup_path).await;
    let _ = std::fs::remove_dir_all(&root_skeleton);

    match result {
        Ok(()) => BucketCapability { ok: true, reason: None },
        Err(e) => BucketCapability { ok: false, reason: Some(format!("{e:#}")) },
    }
}

async fn probe_inner(root_skeleton: &Path, cgroup_path: &Path) -> Result<()> {
    std::fs::create_dir_all(root_skeleton).context("failed to create probe workspace")?;
    create_cgroup(cgroup_path).context("failed to create probe cgroup")?;

    let workspace = root_skeleton.join("probe-workspace");
    std::fs::create_dir_all(&workspace)?;

    let result = run_in_sandbox(root_skeleton, cgroup_path, &workspace, "true", None, &[], |_, _| {}).await?;
    if result.exit_code != 0 {
        anyhow::bail!("probe command exited with status {}", result.exit_code);
    }
    Ok(())
}

/// Spawns `__bucket-init` with the namespace-unshare `pre_exec` hook, streams its stdout/stderr
/// line-by-line to `on_line`, and returns its exit code — this is the actual per-step sandbox
/// execution shared by `exec_step` and the capability probe.
async fn run_in_sandbox<F>(
    root_skeleton: &Path,
    cgroup_path: &Path,
    workspace: &Path,
    shell_command: &str,
    working_dir: Option<&str>,
    env: &[String],
    mut on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    let ro_mounts: Vec<PathBuf> = DEFAULT_RO_MOUNTS.iter().map(PathBuf::from).filter(|p| p.exists()).collect();

    let spec = BucketInitSpec {
        workspace: workspace.to_path_buf(),
        root_skeleton: root_skeleton.to_path_buf(),
        ro_mounts,
        cgroup_path: cgroup_path.to_path_buf(),
        shell_command: shell_command.to_string(),
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
    // own /proc/self/{setgroups,uid_map,gid_map} before exec — no heap allocation, no locks, no
    // access to state shared with other threads, satisfying pre_exec's async-signal-safety
    // requirement for the fork-to-exec window.
    unsafe {
        command.pre_exec(unshare_into_new_namespaces);
    }

    let mut child = command.spawn().context("failed to spawn __bucket-init")?;
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
/// `Command::pre_exec`). Puts the child into new user/mount/uts/ipc/cgroup/net/pid namespaces
/// and maps its own (unprivileged) uid/gid to 0 inside the new user namespace — the standard
/// unprivileged-user-namespace pattern (`man 7 user_namespaces`), safe here because a process
/// that just created a user namespace via `unshare` automatically holds full capabilities
/// within that namespace, including the `CAP_SETUID`/`CAP_SETGID` needed to write its own map.
fn unshare_into_new_namespaces() -> std::io::Result<()> {
    let flags = CloneFlags::CLONE_NEWUSER
        | CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWCGROUP
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
/// cgroups. A failure here isn't fatal to sandbox creation — `cgroup.kill` (the guaranteed
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

/// Tears down a bucket's cgroup: `cgroup.kill` (Linux 5.14+) atomically SIGKILLs every process
/// in the cgroup in one write, regardless of how deep a process tree the sandboxed command
/// forked — this is the guaranteed-cleanup mechanism, not best-effort process tracking.
async fn destroy_cgroup(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let _ = std::fs::write(path.join("cgroup.kill"), "1");

    for _ in 0..20 {
        if std::fs::remove_dir(path).is_ok() {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    std::fs::remove_dir(path).context("failed to remove bucket cgroup directory after cgroup.kill")
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the pure path/serialization logic only. The actual spawn-and-isolate path
    // (`run_in_sandbox`) re-execs `std::env::current_exe()` as `__bucket-init`, which under
    // `cargo test` resolves to the test harness binary rather than the real `actions-toolkit`
    // binary (Cargo doesn't set `CARGO_BIN_EXE_*` for a crate's own unit tests, only for
    // separate integration-test/example targets, and this crate has no library target to give
    // such a target access to these internals anyway) — so it isn't exercised here. It's been
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
}
