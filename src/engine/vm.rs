//! Virtual machines. Each hypervisor is reached through a "transport" that can run a command in
//! the guest with stdin attached: native CLIs where they exist (Lima, Multipass, WSL, Parallels)
//! and SSH for everything else (VirtualBox, VMware, QEMU, Hyper-V, Tart).
//!
//! Commands passed through a transport are always plain words (paths under a slug-only
//! directory), because several transports re-join argv with spaces. Scripts, env vars and the
//! working directory travel inside a generated wrapper script uploaded via stdin.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::{
    apply_probe, find_binary, probe_unix, version_of, Availability, Engine, EngineInfo,
    ExecRequest, Kind, Session, SessionInfo, SessionRequest,
};
use crate::config::{Config, EngineConfig};
use crate::util::{self, shell_quote, Cancel, ExecOutcome};

fn locate(kind: Kind) -> Option<PathBuf> {
    match kind {
        Kind::Lima => find_binary(
            &["limactl"],
            &["/opt/homebrew/bin/limactl", "/usr/local/bin/limactl"],
        ),
        Kind::Multipass => find_binary(
            &["multipass"],
            &[
                "/usr/local/bin/multipass",
                "/snap/bin/multipass",
                r"C:\Program Files\Multipass\bin\multipass.exe",
            ],
        ),
        Kind::Wsl => find_binary(&["wsl"], &[r"C:\Windows\System32\wsl.exe"]),
        Kind::Parallels => find_binary(&["prlctl"], &["/usr/local/bin/prlctl"]),
        Kind::Tart => find_binary(&["tart"], &["/opt/homebrew/bin/tart"]),
        Kind::VirtualBox => find_binary(
            &["VBoxManage", "vboxmanage"],
            &[
                "/Applications/VirtualBox.app/Contents/MacOS/VBoxManage",
                r"C:\Program Files\Oracle\VirtualBox\VBoxManage.exe",
            ],
        ),
        Kind::VMware => find_binary(
            &["vmrun"],
            &[
                "/Applications/VMware Fusion.app/Contents/Library/vmrun",
                r"C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe",
                r"C:\Program Files\VMware\VMware Workstation\vmrun.exe",
            ],
        ),
        Kind::Qemu => find_binary(
            &["qemu-system-x86_64", "qemu-system-aarch64"],
            &[r"C:\Program Files\qemu\qemu-system-x86_64.exe"],
        ),
        Kind::HyperV => {
            let out = util::probe(
                "powershell",
                &[
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "if (Get-Command Get-VM -ErrorAction SilentlyContinue) { 'yes' }",
                ],
            )?;
            out.stdout
                .contains("yes")
                .then(|| PathBuf::from("powershell"))
        }
        _ => None,
    }
}
