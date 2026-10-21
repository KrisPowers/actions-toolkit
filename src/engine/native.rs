//! The built-in engine: runs steps directly on the host inside a throwaway sandbox directory
//! with a scrubbed environment. Zero dependencies and the fastest startup, but it isolates
//! files and environment variables, not the kernel; use a container or VM for untrusted code.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use tempfile::TempDir;

use super::{
    Availability, Engine, EngineInfo, ExecRequest, Kind, Session, SessionInfo, SessionRequest,
};
use crate::util::{self, Cancel, ExecOutcome};

/// Host variables that tools commonly need to function; everything else is dropped.
const PASSTHROUGH: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TZ",
    "SSH_AUTH_SOCK",
    "XDG_RUNTIME_DIR",
    "DOCKER_HOST",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "GOPATH",
    "GOROOT",
    "JAVA_HOME",
    "NVM_DIR",
    "PYENV_ROOT",
    "USERPROFILE",
    "USERNAME",
    "USERDOMAIN",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "COMMONPROGRAMFILES",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "OS",
    "HOMEDRIVE",
    "HOMEPATH",
];

pub(super) fn detect() -> EngineInfo {
    EngineInfo::new(
        Kind::Native,
        Availability::Ready,
        Some(env!("CARGO_PKG_VERSION").to_string()),
        "sandboxed directory + clean environment on this machine".to_string(),
    )
}

pub struct NativeEngine;

fn host_os() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

fn host_arch() -> String {
    match env::consts::ARCH {
        "x86_64" => "X64".into(),
        "aarch64" => "ARM64".into(),
        "x86" => "X86".into(),
        "arm" => "ARM".into(),
        other => other.to_uppercase(),
    }
}
