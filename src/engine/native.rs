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

fn default_shell() -> String {
    let pick = |names: &[&str]| {
        names
            .iter()
            .find(|n| which::which(n).is_ok())
            .map(|s| s.to_string())
    };
    if cfg!(windows) {
        pick(&["pwsh", "powershell"]).unwrap_or_else(|| "cmd".into())
    } else {
        pick(&["bash"]).unwrap_or_else(|| "sh".into())
    }
}

impl Engine for NativeEngine {
    fn kind(&self) -> Kind {
        Kind::Native
    }

    fn start(&self, req: &SessionRequest, log: &mut dyn FnMut(String)) -> Result<Box<dyn Session>> {
        let dir = tempfile::Builder::new().prefix("maverick-").tempdir()?;
        let temp = dir.path().join("runner");
        fs::create_dir_all(&temp)?;
        let workspace = if req.bind {
            log(format!(
                "Running in place in {} (--bind)",
                req.source.display()
            ));
            req.source.to_path_buf()
        } else {
            let ws = dir.path().join("workspace");
            let files = util::copy_workspace(req.source, &ws)?;
            log(format!(
                "Copied {files} project files into sandbox {}",
                ws.display()
            ));
            ws
        };
        let host_env: Vec<(String, String)> = env::vars()
            .filter(|(k, _)| PASSTHROUGH.contains(&k.to_ascii_uppercase().as_str()))
            .collect();
        let temp_s = temp.to_string_lossy().into_owned();
        let info = SessionInfo {
            workspace: workspace.to_string_lossy().into_owned(),
            tool_cache: format!("{temp_s}{}_tool", std::path::MAIN_SEPARATOR),
            temp: temp_s,
            os: host_os(),
            arch: host_arch(),
            path: env::var("PATH").unwrap_or_default(),
            default_shell: default_shell(),
        };
        log(format!(
            "Host: {} {} · default shell: {}",
            info.os, info.arch, info.default_shell
        ));
        Ok(Box::new(NativeSession {
            info,
            host_env,
            dir: Some(dir),
        }))
    }
}
