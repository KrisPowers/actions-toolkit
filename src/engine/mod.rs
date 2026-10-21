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

#[derive(Clone, Debug)]
pub struct EngineInfo {
    pub kind: Kind,
    pub availability: Availability,
    pub version: Option<String>,
    /// Extra context: how to fix it, or which instance/target is used.
    pub note: Option<String>,
}

impl EngineInfo {
    fn new(
        kind: Kind,
        availability: Availability,
        version: Option<String>,
        note: impl Into<Option<String>>,
    ) -> Self {
        Self {
            kind,
            availability,
            version,
            note: note.into(),
        }
    }

    fn missing(kind: Kind) -> Self {
        Self::new(kind, Availability::Missing, None, None)
    }

    pub fn is_ready(&self) -> bool {
        self.availability == Availability::Ready
    }

    pub fn status_label(&self) -> &'static str {
        match self.availability {
            Availability::Ready => "ready",
            Availability::NotRunning => "not running",
            Availability::NeedsSetup => "needs setup",
            Availability::Missing => "not installed",
        }
    }
}

pub fn detect(kind: Kind, cfg: &Config) -> EngineInfo {
    match kind.family() {
        Family::Container => container::detect(kind),
        Family::VirtualMachine => vm::detect(kind, cfg),
        Family::BuiltIn => native::detect(),
    }
}

/// Probes every engine relevant to this OS in parallel.
pub fn detect_all(cfg: &Config) -> Vec<EngineInfo> {
    let handles: Vec<_> = Kind::ALL
        .into_iter()
        .filter(|k| k.on_this_host())
        .map(|k| {
            let cfg = cfg.clone();
            thread::spawn(move || detect(k, &cfg))
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

/// The engine to use when none is configured: a running container engine, else the built-in one.
pub fn auto_select(cfg: &Config) -> Kind {
    [Kind::Docker, Kind::Podman]
        .into_iter()
        .find(|&k| detect(k, cfg).is_ready())
        .unwrap_or(Kind::Native)
}

/// Instantiates an engine, failing with an actionable message if it isn't usable.
pub fn build(kind: Kind, cfg: &Config) -> Result<Arc<dyn Engine>> {
    let info = detect(kind, cfg);
    if !info.is_ready() {
        let hint = info
            .note
            .as_ref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        bail!("{} is {}{hint}", kind.name(), info.status_label());
    }
    Ok(match kind.family() {
        Family::Container => Arc::new(container::ContainerEngine::new(kind)?),
        Family::VirtualMachine => Arc::new(vm::VmEngine::new(kind, cfg)?),
        Family::BuiltIn => Arc::new(native::NativeEngine),
    })
}

pub trait Engine: Send + Sync {
    fn kind(&self) -> Kind;

    /// Whether jobs need a container image (resolved from `runs-on`).
    fn uses_images(&self) -> bool {
        self.kind().family() == Family::Container
    }

    /// Provisions an isolated environment containing a copy of `req.source`.
    fn start(&self, req: &SessionRequest, log: &mut dyn FnMut(String)) -> Result<Box<dyn Session>>;
}

pub struct SessionRequest<'a> {
    /// Unique, slug-safe identifier for this job run.
    pub id: String,
    pub source: &'a Path,
    pub image: Option<String>,
    pub container_env: Vec<(String, String)>,
    pub volumes: Vec<String>,
    pub options: Option<String>,
    /// Mount the project in place instead of copying it.
    pub bind: bool,
    /// Always pull images.
    pub pull: bool,
    pub cancel: Cancel,
}

pub struct ExecRequest {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub timeout: Option<Duration>,
}

/// Facts about a running session; all paths are as seen from inside the environment.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub workspace: String,
    pub temp: String,
    pub tool_cache: String,
    pub os: &'static str,
    pub arch: String,
    /// Default `PATH` inside the environment.
    pub path: String,
    pub default_shell: String,
}

impl SessionInfo {
    pub fn linux(workspace: &str, temp: &str) -> Self {
        Self {
            workspace: workspace.to_string(),
            temp: temp.to_string(),
            tool_cache: format!("{temp}/_tool"),
            os: "Linux",
            arch: "X64".into(),
            path: "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into(),
            default_shell: "sh".into(),
        }
    }

    fn windows(&self) -> bool {
        self.os == "Windows"
    }

    pub fn join(&self, base: &str, rel: &str) -> String {
        let sep = if self.windows() { '\\' } else { '/' };
        let rel = rel.trim_start_matches("./");
        if rel.is_empty() || rel == "." {
            return base.to_string();
        }
        format!("{}{sep}{}", base.trim_end_matches(['/', '\\']), rel)
    }

    pub fn is_absolute(&self, p: &str) -> bool {
        p.starts_with('/')
            || (self.windows() && (p.starts_with('\\') || p.chars().nth(1) == Some(':')))
    }

    pub fn path_separator(&self) -> &'static str {
        if self.windows() {
            ";"
        } else {
            ":"
        }
    }
}

pub trait Session: Send {
    fn info(&self) -> &SessionInfo;

    fn exec(
        &mut self,
        req: &ExecRequest,
        cancel: &Cancel,
        out: &mut dyn FnMut(String),
    ) -> Result<ExecOutcome>;

    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<()>;

    /// Reads a file; a missing file reads as empty.
    fn read_file(&mut self, path: &str) -> Result<String>;

    /// Copies a host directory into the environment at `dest`.
    fn copy_dir(&mut self, host: &Path, dest: &str) -> Result<()>;

    fn teardown(&mut self, log: &mut dyn FnMut(String));
}

const PROBE: &str = "printf '%s\\n' \"$PATH\"\nif command -v bash >/dev/null 2>&1; then echo bash; else echo sh; fi\nuname -m\n";

/// Discovers PATH, default shell and architecture inside a Unix-like session.
fn probe_unix(session: &mut dyn Session) -> Result<(String, String, String)> {
    let info = session.info().clone();
    let script = format!("{}/probe.sh", info.temp);
    session.write_file(&script, PROBE.as_bytes())?;
    let req = ExecRequest {
        argv: vec!["sh".into(), script],
        env: Vec::new(),
        cwd: info.workspace.clone(),
        timeout: Some(Duration::from_secs(60)),
    };
    let mut lines = Vec::new();
    let outcome = session.exec(&req, &Cancel::new(), &mut |l| lines.push(l))?;
    if !outcome.success() || lines.len() < 3 {
        bail!("environment probe failed: {}", lines.join(" / "));
    }
    let n = lines.len();
    let arch = match lines[n - 1].trim() {
        "x86_64" | "amd64" => "X64".to_string(),
        "aarch64" | "arm64" => "ARM64".to_string(),
        other => other.to_uppercase(),
    };
    Ok((lines[n - 3].clone(), lines[n - 2].trim().to_string(), arch))
}
