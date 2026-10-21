//! Isolation engines: where a job's steps actually execute.
//!
//! Every engine hands out a [`Session`]: an isolated environment holding a copy of the
//! project, in which the runner can execute commands and exchange small files.

mod container;
mod native;
mod vm;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::config::Config;
use crate::util::{self, Cancel, ExecOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Docker,
    Podman,
    Lima,
    Multipass,
    Wsl,
    Parallels,
    Tart,
    VirtualBox,
    VMware,
    Qemu,
    HyperV,
    Native,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Container,
    VirtualMachine,
    BuiltIn,
}

impl Family {
    pub fn label(self) -> &'static str {
        match self {
            Family::Container => "container",
            Family::VirtualMachine => "vm",
            Family::BuiltIn => "built-in",
        }
    }
}

impl Kind {
    pub const ALL: [Kind; 12] = [
        Kind::Docker,
        Kind::Podman,
        Kind::Lima,
        Kind::Multipass,
        Kind::Wsl,
        Kind::Parallels,
        Kind::Tart,
        Kind::VirtualBox,
        Kind::VMware,
        Kind::Qemu,
        Kind::HyperV,
        Kind::Native,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Docker => "docker",
            Kind::Podman => "podman",
            Kind::Lima => "lima",
            Kind::Multipass => "multipass",
            Kind::Wsl => "wsl",
            Kind::Parallels => "parallels",
            Kind::Tart => "tart",
            Kind::VirtualBox => "virtualbox",
            Kind::VMware => "vmware",
            Kind::Qemu => "qemu",
            Kind::HyperV => "hyperv",
            Kind::Native => "native",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Docker => "Docker",
            Kind::Podman => "Podman",
            Kind::Lima => "Lima",
            Kind::Multipass => "Multipass",
            Kind::Wsl => "WSL",
            Kind::Parallels => "Parallels",
            Kind::Tart => "Tart",
            Kind::VirtualBox => "VirtualBox",
            Kind::VMware => "VMware",
            Kind::Qemu => "QEMU",
            Kind::HyperV => "Hyper-V",
            Kind::Native => "Maverick Native",
        }
    }

    pub fn family(self) -> Family {
        match self {
            Kind::Docker | Kind::Podman => Family::Container,
            Kind::Native => Family::BuiltIn,
            _ => Family::VirtualMachine,
        }
    }

    /// Whether this engine can exist on the current OS at all.
    pub fn on_this_host(self) -> bool {
        let windows_only = matches!(self, Kind::Wsl | Kind::HyperV);
        let mac_only = matches!(self, Kind::Parallels | Kind::Tart);
        (!windows_only || cfg!(windows)) && (!mac_only || cfg!(target_os = "macos"))
    }

    pub fn parse(s: &str) -> Option<Kind> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|k| k.id() == s)
            .or(match s.as_str() {
                "hyper-v" => Some(Kind::HyperV),
                "vbox" => Some(Kind::VirtualBox),
                "builtin" | "built-in" | "maverick" | "host" => Some(Kind::Native),
                _ => None,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Ready,
    NotRunning,
    NeedsSetup,
    Missing,
}
