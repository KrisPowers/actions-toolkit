//! Seccomp-bpf policy applied to PID 1 of every bucket sandbox, on top of (not instead of) the
//! namespace isolation and capability drop already in place. This is a denylist, not an
//! allowlist: workflow steps are arbitrary shell commands that can need almost any "normal"
//! syscall, so an allowlist would be a constant source of breakage. The syscalls blocked here
//! are ones capability-dropping alone doesn't reliably cover (e.g. unprivileged nested
//! `unshare`/`clone(CLONE_NEWUSER)` needs no capability at all on hosts where unprivileged user
//! namespaces are permitted) or that have no legitimate reason to be called from inside a
//! sandboxed workflow step regardless of capabilities.

use anyhow::{Context, Result};
use seccompiler::{apply_filter, SeccompAction, SeccompFilter, SeccompRule};
use std::collections::BTreeMap;

const DENIED_SYSCALLS: &[i64] = &[
    libc::SYS_unshare,
    libc::SYS_setns,
    libc::SYS_mount,
    libc::SYS_umount2,
    libc::SYS_pivot_root,
    libc::SYS_chroot,
    libc::SYS_reboot,
    libc::SYS_kexec_load,
    libc::SYS_init_module,
    libc::SYS_finit_module,
    libc::SYS_delete_module,
    libc::SYS_ptrace,
    libc::SYS_process_vm_readv,
    libc::SYS_process_vm_writev,
    libc::SYS_bpf,
    libc::SYS_perf_event_open,
    libc::SYS_add_key,
    libc::SYS_request_key,
    libc::SYS_keyctl,
    libc::SYS_swapon,
    libc::SYS_swapoff,
    libc::SYS_acct,
    libc::SYS_quotactl,
    libc::SYS_syslog,
    libc::SYS_personality,
];

/// EPERM, returned to the calling process for every denied syscall rather than killing it
/// outright, so a step that happens to touch one of these fails with an ordinary error instead
/// of being silently SIGSYS'd.
const ERRNO_EPERM: u64 = libc::EPERM as u64;

pub(crate) fn install() -> Result<()> {
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    for &sysno in DENIED_SYSCALLS {
        rules.insert(sysno, vec![]);
    }

    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(ERRNO_EPERM as u32),
        std::env::consts::ARCH.try_into().context("unsupported seccomp target arch")?,
    )
    .context("failed to build seccomp filter")?;

    let program: seccompiler::BpfProgram = filter.try_into().context("failed to compile seccomp filter")?;
    apply_filter(&program).context("failed to apply seccomp filter")
}
