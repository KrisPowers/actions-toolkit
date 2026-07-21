//! The `__bucket-init` re-exec stage (Linux only). The host process is multi-threaded tokio,
//! and the kernel refuses `unshare(CLONE_NEWUSER)` from a multi-threaded process, so namespace
//! setup can't happen there directly. Instead the host spawns this same binary again under the
//! hidden `__bucket-init` subcommand with a `pre_exec` hook that unshares namespaces in the
//! freshly-forked, single-threaded child (see `linux::spawn_bucket_init` for that half). By the
//! time `run()` below executes, this process is already inside its own user/mount/uts/ipc/
//! cgroup/net namespaces, mapped to uid 0 within them, but still in the *host's* PID namespace
//! (`unshare(CLONE_NEWPID)` only affects this process's own future children, per unshare(2)).
//!
//! From here this process:
//! 1. joins the pre-created cgroup (must happen before pivot_root severs the host cgroupfs path)
//! 2. assembles the sandbox root (bind-mount workspace + curated read-only host dirs) and
//!    `pivot_root`s into it, so anything not explicitly mounted is invisible at the path level
//! 3. forks once more, this fork is the one that actually lands inside the new PID namespace,
//!    becoming its PID 1
//! 4. PID 1 mounts a fresh `/proc`, installs a seccomp filter, drops all capabilities, and
//!    `execve`s the step's shell command
//! 5. this process just waits on PID 1 and exits with its status. PID 1 dying for any reason
//!    (including this process itself being killed, via `PR_SET_PDEATHSIG`) tears down every
//!    remaining task in the namespace automatically

use std::ffi::CString;
use std::path::Path;

use anyhow::{Context, Result};
use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{chdir, fork, ForkResult, Pid};

use super::BucketInitSpec;
use atk_config::BucketInitArgs;

pub fn run(args: BucketInitArgs) -> Result<i32> {
    let spec_bytes = std::fs::read(&args.spec_path).context("failed to read bucket init spec")?;
    let spec: BucketInitSpec = serde_json::from_slice(&spec_bytes).context("failed to parse bucket init spec")?;

    join_cgroup(&spec.cgroup_path).context("failed to join cgroup")?;
    // Only safe to unshare the cgroup namespace *after* joining the target cgroup by its
    // absolute host path above (see the note on `linux::unshare_into_new_namespaces`); from
    // here on this process's cgroupfs view is rooted at the cgroup it just joined.
    nix::sched::unshare(nix::sched::CloneFlags::CLONE_NEWCGROUP).context("failed to unshare cgroup namespace")?;
    build_sandbox_root(&spec).context("failed to assemble sandbox root filesystem")?;

    // SAFETY: this process is single-threaded (a freshly re-exec'd binary whose main() dispatches
    // straight into this function, no tokio runtime started), so `fork()` is sound here.
    match unsafe { fork() }.context("failed to fork sandbox init process")? {
        ForkResult::Parent { child } => Ok(wait_for_child(child)),
        ForkResult::Child => {
            // Anything below this point is PID 1 of the new PID namespace. If it returns or
            // panics, exit immediately rather than unwinding back into a forked copy of the
            // parent's control flow.
            match run_pid1(&spec) {
                Ok(()) => unreachable!("run_pid1 only returns on error; success replaces the process image"),
                Err(e) => {
                    tracing::error!(error = %e, "bucket PID 1 setup failed");
                    std::process::exit(127);
                }
            }
        }
    }
}

fn wait_for_child(child: Pid) -> i32 {
    match waitpid(child, None) {
        Ok(WaitStatus::Exited(_, code)) => code,
        Ok(WaitStatus::Signaled(_, signal, _)) => 128 + signal as i32,
        Ok(_) | Err(_) => 1,
    }
}

fn join_cgroup(cgroup_path: &Path) -> Result<()> {
    let pid = std::process::id();
    std::fs::write(cgroup_path.join("cgroup.procs"), pid.to_string())
        .with_context(|| format!("failed to join cgroup at {}", cgroup_path.display()))
}

/// Bind-mounts the workspace (read-write) and the curated read-only host directories into
/// `spec.root_skeleton`, then `pivot_root`s into it. Everything not explicitly bind-mounted here
/// is unreachable afterward, not just access-denied: `pivot_root` + detaching the old root make
/// unmounted paths resolve to `ENOENT`.
fn build_sandbox_root(spec: &BucketInitSpec) -> Result<()> {
    let new_root = &spec.root_skeleton;

    // A bind-mount of the root skeleton onto itself makes it a mount point in its own right,
    // which pivot_root requires (the new root must not be on the same mount as its parent).
    mount(Some(new_root), new_root, None::<&str>, MsFlags::MS_BIND | MsFlags::MS_REC, None::<&str>)
        .context("failed to self-bind-mount sandbox root")?;

    let workspace_target = new_root.join("workspace");
    std::fs::create_dir_all(&workspace_target)?;
    mount(Some(&spec.workspace), &workspace_target, None::<&str>, MsFlags::MS_BIND, None::<&str>)
        .context("failed to bind-mount workspace into sandbox")?;

    for host_path in &spec.ro_mounts {
        let Ok(relative) = host_path.strip_prefix("/") else { continue };
        let target = new_root.join(relative);
        if let Err(e) = std::fs::create_dir_all(&target) {
            tracing::warn!(error = %e, path = %host_path.display(), "skipping ro mount, could not create target dir");
            continue;
        }
        if let Err(e) = mount(Some(host_path), &target, None::<&str>, MsFlags::MS_BIND | MsFlags::MS_REC, None::<&str>) {
            tracing::warn!(error = %e, path = %host_path.display(), "skipping ro mount, bind failed");
            continue;
        }
        // A single-call MS_BIND|MS_RDONLY is ignored by the kernel for bind mounts; the
        // read-only flag has to be applied with a separate remount.
        if let Err(e) = mount(
            None::<&str>,
            &target,
            None::<&str>,
            MsFlags::MS_BIND | MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY | MsFlags::MS_REC,
            None::<&str>,
        ) {
            tracing::warn!(error = %e, path = %host_path.display(), "failed to remount ro mount read-only");
        }
    }

    std::fs::create_dir_all(new_root.join("proc"))?;
    let put_old = new_root.join(".put_old");
    std::fs::create_dir_all(&put_old)?;

    chdir(new_root).context("failed to chdir into sandbox root")?;
    nix::unistd::pivot_root(".", ".put_old").context("pivot_root failed")?;
    chdir("/").context("failed to chdir to new root")?;
    umount2("/.put_old", MntFlags::MNT_DETACH).context("failed to detach old root")?;
    let _ = std::fs::remove_dir("/.put_old");

    Ok(())
}

/// Runs as PID 1 of the new PID namespace: mount a fresh `/proc` (only safe now that this
/// process is actually inside the new namespace), lock the process down (seccomp, capabilities,
/// no-new-privs, parent-death signal), then exec the step's command. Only returns on error.
fn run_pid1(spec: &BucketInitSpec) -> Result<()> {
    mount(Some("proc"), "/proc", Some("proc"), MsFlags::empty(), None::<&str>).context("failed to mount /proc")?;

    super::seccomp_policy::install().context("failed to install seccomp filter")?;
    drop_all_capabilities().context("failed to drop capabilities")?;

    nix::sys::prctl::set_no_new_privs().context("failed to set no_new_privs")?;
    // SAFETY: PR_SET_PDEATHSIG with no other side effects; if unsupported this just no-ops.
    unsafe {
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
    }

    let working_dir = spec.working_dir.as_deref().unwrap_or("/workspace");
    chdir(working_dir).with_context(|| format!("failed to chdir into '{working_dir}'"))?;

    exec_shell_command(spec.shell.as_deref(), &spec.shell_command, &spec.env)
}

/// Resolves a step's `shell:` override into the absolute path `execve` needs (unlike `execvp`,
/// `execve` doesn't search `$PATH`) and the flag that hands it the command inline. Defaults to
/// `bash` when unset, matching real GitHub Actions' Linux runner default; `sh` is still
/// recognized for parity with the Docker exec path. Anything else is passed through as a literal
/// path, best-effort.
fn resolve_shell(shell: Option<&str>) -> (String, &'static str) {
    match shell.map(str::to_ascii_lowercase).as_deref() {
        None | Some("bash") => ("/bin/bash".to_string(), "-c"),
        Some("sh") => ("/bin/sh".to_string(), "-c"),
        Some(other) => (other.to_string(), "-c"),
    }
}

fn drop_all_capabilities() -> Result<()> {
    use caps::CapSet;
    for set in [CapSet::Ambient, CapSet::Inheritable, CapSet::Effective, CapSet::Permitted, CapSet::Bounding] {
        // Ambient/Inheritable clears can fail harmlessly on some kernels; only Bounding and
        // Effective/Permitted are load-bearing for actually removing privilege.
        let _ = caps::clear(None, set);
    }
    Ok(())
}

fn exec_shell_command(shell: Option<&str>, shell_command: &str, env: &[String]) -> Result<()> {
    let (program, flag) = resolve_shell(shell);
    let args = [CString::new(program.as_str())?, CString::new(flag)?, CString::new(shell_command)?];

    let mut env_vars: Vec<String> = env.to_vec();
    if !env_vars.iter().any(|e| e.starts_with("PATH=")) {
        env_vars.push("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string());
    }
    let env_cstrings: Vec<CString> = env_vars.iter().map(|e| CString::new(e.as_str())).collect::<Result<_, _>>()?;

    nix::unistd::execve(&args[0], &args, &env_cstrings).context("failed to exec step command")?;
    unreachable!("execve only returns on error, which is mapped to Err above");
}
