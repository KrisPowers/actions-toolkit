//! Where a job's steps actually execute: one Docker container per job.
//!
//! The engine hands out a [`Session`]: an isolated environment holding a copy of the
//! project, in which the runner can execute commands and exchange small files.

mod container;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::util::{self, Cancel, ExecOutcome};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Ready,
    NotRunning,
    /// Running, but with Windows containers, which can't run the Linux job images.
    WrongMode,
    Missing,
}

/// What `detect` found out about Docker on this machine.
#[derive(Clone, Debug)]
pub struct EngineInfo {
    pub availability: Availability,
    pub version: Option<String>,
    /// How to fix it when it isn't ready.
    pub note: Option<String>,
}

impl EngineInfo {
    pub fn is_ready(&self) -> bool {
        self.availability == Availability::Ready
    }

    pub fn status_label(&self) -> &'static str {
        match self.availability {
            Availability::Ready => "ready",
            Availability::NotRunning => "not running",
            Availability::WrongMode => "using Windows containers",
            Availability::Missing => "not installed",
        }
    }
}

/// Every run happens in Docker; this is its display name.
pub const NAME: &str = "Docker";

/// Checks whether Docker is installed and its daemon answers.
pub fn detect() -> EngineInfo {
    container::detect()
}

/// Instantiates the Docker engine, failing with an actionable message if it isn't usable.
pub fn build() -> Result<Arc<dyn Engine>> {
    let info = detect();
    if !info.is_ready() {
        let hint = info
            .note
            .as_ref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();
        bail!("{NAME} is {}{hint}", info.status_label());
    }
    Ok(Arc::new(container::ContainerEngine::new()?))
}

pub trait Engine: Send + Sync {
    fn name(&self) -> &'static str;

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
    /// Host folder holding the project's `actions/cache` entries, shared with the job.
    pub cache: Option<PathBuf>,
    pub cancel: Cancel,
}

#[derive(Default)]
pub struct ExecRequest {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub timeout: Option<Duration>,
    /// Run in a new container from this image, sharing the job's workspace, instead of in
    /// the job's environment (Docker actions). `argv` are then the entrypoint's arguments.
    pub image: Option<String>,
    pub entrypoint: Option<String>,
}

/// Where a Docker action's image comes from.
pub enum ActionImage {
    /// A published image, pulled when it isn't present.
    Pull(String),
    /// Built from this Dockerfile, with its folder as the build context.
    Build(PathBuf),
}

/// Facts about a running session; all paths are as seen from inside the environment.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub workspace: String,
    pub temp: String,
    pub tool_cache: String,
    /// The shared `actions/cache` folder, when there is one.
    pub cache: Option<String>,
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
            cache: None,
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

    /// The workspace as a host folder, which `hashFiles()` reads.
    fn host_workspace(&self) -> &Path;

    fn exec(
        &mut self,
        req: &ExecRequest,
        cancel: &Cancel,
        out: &mut dyn FnMut(String),
    ) -> Result<ExecOutcome>;

    /// Makes a Docker action's image available and returns the name to run it by.
    fn prepare_image(
        &mut self,
        image: &ActionImage,
        cancel: &Cancel,
        log: &mut dyn FnMut(String),
    ) -> Result<String>;

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
        ..Default::default()
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

fn apply_probe(info: &mut SessionInfo, (path, shell, arch): (String, String, String)) {
    info.path = path;
    info.default_shell = shell;
    info.arch = arch;
}

/// Finds a binary on PATH or at well-known install locations (`~` is expanded).
pub(crate) fn find_binary(names: &[&str], extra: &[&str]) -> Option<PathBuf> {
    names
        .iter()
        .find_map(|n| which::which(n).ok())
        .or_else(|| extra.iter().map(|p| expand_home(p)).find(|p| p.is_file()))
}

pub(crate) fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(p),
    }
}

fn version_of(bin: &Path, args: &[&str]) -> Option<String> {
    let out = util::probe(bin, args)?;
    util::extract_version(&out.stdout).or_else(|| util::extract_version(&out.stderr))
}
