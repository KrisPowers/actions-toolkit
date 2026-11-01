//! Docker: one long-lived container per job, steps run via `exec`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tempfile::TempDir;

use super::{
    apply_probe, find_binary, probe_unix, version_of, ActionImage, Availability, Engine,
    EngineInfo, ExecRequest, Session, SessionInfo, SessionRequest,
};
use crate::util::{self, Cancel, ExecOutcome};

const WORKSPACE: &str = "/github/workspace";
const RUNNER: &str = "/github/runner";
const CACHE: &str = "/github/cache";
const ARTIFACTS: &str = "/github/artifacts";

fn locate() -> Option<PathBuf> {
    find_binary(
        &["docker"],
        &[
            "/usr/local/bin/docker",
            "/opt/homebrew/bin/docker",
            "~/.docker/bin/docker",
            "~/.orbstack/bin/docker",
            "~/.rd/bin/docker",
            "/Applications/Docker.app/Contents/Resources/bin/docker",
            r"C:\Program Files\Docker\Docker\resources\bin\docker.exe",
        ],
    )
}

pub(super) fn detect() -> EngineInfo {
    let Some(bin) = locate() else {
        return EngineInfo {
            availability: Availability::Missing,
            version: None,
            note: Some("install Docker Engine or Docker Desktop".into()),
        };
    };
    let version = version_of(&bin, &["--version"]);
    // `info` talks to the daemon, so it tells us whether containers can actually run, and
    // whether they are Linux containers, which every job image is.
    let os = util::probe(&bin, &["info", "--format", "{{.OSType}}"])
        .filter(|o| o.success)
        .map(|o| o.stdout.trim().to_string());
    let (availability, note) = match os.as_deref() {
        None => (
            Availability::NotRunning,
            "start Docker Desktop (or the docker daemon)",
        ),
        Some("linux") => (Availability::Ready, ""),
        Some(_) => (
            Availability::WrongMode,
            "switch Docker Desktop to Linux containers",
        ),
    };
    EngineInfo {
        availability,
        version,
        note: (!note.is_empty()).then(|| note.into()),
    }
}

pub struct ContainerEngine {
    binary: PathBuf,
}

impl ContainerEngine {
    pub fn new() -> Result<Self> {
        let binary = locate().context("Docker is not installed")?;
        Ok(Self { binary })
    }

    fn cmd(&self) -> Command {
        Command::new(&self.binary)
    }
}

fn ensure_image(
    binary: &Path,
    image: &str,
    pull: bool,
    cancel: &Cancel,
    log: &mut dyn FnMut(String),
) -> Result<()> {
    if !pull {
        let mut inspect = Command::new(binary);
        inspect.args(["image", "inspect", image]);
        if util::capture(inspect, Duration::from_secs(30)).is_ok_and(|o| o.success) {
            return Ok(());
        }
    }
    log(format!("Pulling image {image}"));
    let mut cmd = Command::new(binary);
    cmd.args(["pull", image]);
    let outcome = util::stream(cmd, cancel, None, log)?;
    if !outcome.success() {
        bail!("failed to pull image {image}");
    }
    Ok(())
}

fn mount(cmd: &mut Command, host: &Path, target: &str) {
    cmd.arg("--mount").arg(format!(
        "type=bind,source={},target={target}",
        host.display()
    ));
}

impl Engine for ContainerEngine {
    fn name(&self) -> &'static str {
        super::NAME
    }

    fn start(&self, req: &SessionRequest, log: &mut dyn FnMut(String)) -> Result<Box<dyn Session>> {
        let image = req
            .image
            .clone()
            .context("no container image resolved for this job")?;
        let dir = tempfile::Builder::new().prefix("atk-").tempdir()?;
        let runner_host = dir.path().join("runner");
        fs::create_dir_all(&runner_host)?;
        let workspace_host = if req.bind {
            log(format!(
                "Mounting {} in place (--bind)",
                req.source.display()
            ));
            req.source.to_path_buf()
        } else {
            let ws = dir.path().join("workspace");
            let files = util::copy_workspace(req.source, &ws)?;
            log(format!(
                "Copied {files} project files into an isolated workspace"
            ));
            ws
        };

        ensure_image(&self.binary, &image, req.pull, &req.cancel, log)?;

        let name = format!("atk-{}", req.id);
        let mut rm = self.cmd();
        rm.args(["rm", "-f", &name]);
        let _ = util::capture(rm, Duration::from_secs(30));

        let mut cmd = self.cmd();
        cmd.args(["run", "-d", "--name", &name, "--label", "dev.atk.run=1"]);
        mount(&mut cmd, &workspace_host, WORKSPACE);
        mount(&mut cmd, &runner_host, RUNNER);
        if let Some(cache) = &req.cache {
            fs::create_dir_all(cache)?;
            mount(&mut cmd, cache, CACHE);
        }
        if let Some(artifacts) = &req.artifacts {
            fs::create_dir_all(artifacts)?;
            mount(&mut cmd, artifacts, ARTIFACTS);
        }
        cmd.args(["-w", WORKSPACE]);
        for (k, v) in &req.container_env {
            cmd.arg("-e").arg(format!("{k}={v}"));
        }
        for v in &req.volumes {
            cmd.arg("-v").arg(v);
        }
        if let Some(opts) = &req.options {
            cmd.args(opts.split_whitespace());
        }
        cmd.args(["--entrypoint", "tail", &image, "-f", "/dev/null"]);
        let out = util::capture(cmd, Duration::from_secs(120))?;
        if !out.success {
            bail!("could not start container: {}", out.stderr.trim());
        }
        log(format!("Started container {name} from {image}"));

        let mut session = ContainerSession {
            binary: self.binary.clone(),
            name,
            bind: req.bind,
            pull: req.pull,
            workspace_host,
            runner_host,
            info: SessionInfo {
                cache: req.cache.as_ref().map(|_| CACHE.to_string()),
                artifacts: req.artifacts.as_ref().map(|_| ARTIFACTS.to_string()),
                ..SessionInfo::linux(WORKSPACE, RUNNER)
            },
            dir: Some(dir),
            removed: false,
        };
        let probe = probe_unix(&mut session)?;
        apply_probe(&mut session.info, probe);
        Ok(Box::new(session))
    }
}

struct ContainerSession {
    binary: PathBuf,
    name: String,
    bind: bool,
    pull: bool,
    workspace_host: PathBuf,
    runner_host: PathBuf,
    info: SessionInfo,
    dir: Option<TempDir>,
    removed: bool,
}

impl ContainerSession {
    /// Maps an in-container path onto the bind-mounted host directory.
    fn host_path(&self, path: &str) -> Result<PathBuf> {
        let map = |prefix: &str, host: &Path| {
            path.strip_prefix(prefix)
                .filter(|rest| rest.is_empty() || rest.starts_with('/'))
                .map(|rest| host.join(rest.trim_start_matches('/')))
        };
        map(RUNNER, &self.runner_host)
            .or_else(|| map(WORKSPACE, &self.workspace_host))
            .with_context(|| format!("{path} is outside the shared directories"))
    }

    fn remove(&mut self) {
        if self.removed {
            return;
        }
        self.removed = true;
        let mut cmd = Command::new(&self.binary);
        cmd.args(["rm", "-f", &self.name]);
        let _ = util::capture(cmd, Duration::from_secs(30));
    }
}

impl Session for ContainerSession {
    fn info(&self) -> &SessionInfo {
        &self.info
    }

    fn host_workspace(&self) -> &Path {
        &self.workspace_host
    }

    fn exec(
        &mut self,
        req: &ExecRequest,
        cancel: &Cancel,
        out: &mut dyn FnMut(String),
    ) -> Result<ExecOutcome> {
        let Some(image) = &req.image else {
            let mut cmd = Command::new(&self.binary);
            cmd.args(["exec", "-w", &req.cwd]);
            for (k, v) in &req.env {
                cmd.arg("-e").arg(format!("{k}={v}"));
            }
            cmd.arg(&self.name).args(&req.argv);
            return util::stream(cmd, cancel, req.timeout, out);
        };
        // A Docker action: a sibling container on the same shared directories, so workspace
        // changes and step files land where the job's later steps see them.
        let name = format!("{}-action", self.name);
        let mut cmd = Command::new(&self.binary);
        cmd.args(["run", "--rm", "--name", &name, "--label", "dev.atk.run=1"]);
        cmd.args(["-w", &req.cwd]);
        mount(&mut cmd, &self.workspace_host, WORKSPACE);
        mount(&mut cmd, &self.runner_host, RUNNER);
        for (k, v) in &req.env {
            cmd.arg("-e").arg(format!("{k}={v}"));
        }
        if let Some(entrypoint) = &req.entrypoint {
            cmd.args(["--entrypoint", entrypoint.as_str()]);
        }
        cmd.arg(image).args(&req.argv);
        let outcome = util::stream(cmd, cancel, req.timeout, out)?;
        if outcome.cancelled || outcome.timed_out {
            // Stopping the client leaves the container running.
            let mut rm = Command::new(&self.binary);
            rm.args(["rm", "-f", &name]);
            let _ = util::capture(rm, Duration::from_secs(30));
        }
        Ok(outcome)
    }

    fn prepare_image(
        &mut self,
        image: &ActionImage,
        cancel: &Cancel,
        log: &mut dyn FnMut(String),
    ) -> Result<String> {
        let dockerfile = match image {
            ActionImage::Pull(name) => {
                ensure_image(&self.binary, name, self.pull, cancel, log)?;
                return Ok(name.clone());
            }
            ActionImage::Build(dockerfile) => dockerfile,
        };
        let context = dockerfile
            .parent()
            .context("the Dockerfile has no folder")?;
        let tag = format!(
            "action:{:016x}",
            util::hash(dockerfile.to_string_lossy().as_bytes())
        );
        log(format!("Building {} as {tag}", dockerfile.display()));
        let mut cmd = Command::new(&self.binary);
        cmd.args(["build", "-t", &tag]);
        if self.pull {
            cmd.arg("--pull");
        }
        cmd.arg("-f").arg(dockerfile).arg(context);
        if !util::stream(cmd, cancel, None, log)?.success() {
            bail!("failed to build {}", dockerfile.display());
        }
        Ok(tag)
    }

    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let host = self.host_path(path)?;
        if let Some(parent) = host.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(host, data)?;
        Ok(())
    }

    fn read_file(&mut self, path: &str) -> Result<String> {
        Ok(fs::read_to_string(self.host_path(path)?).unwrap_or_default())
    }

    fn copy_dir(&mut self, host: &Path, dest: &str) -> Result<()> {
        util::copy_tree(host, &self.host_path(dest)?)?;
        Ok(())
    }

    fn teardown(&mut self, log: &mut dyn FnMut(String)) {
        // On Linux, files created as root inside the container can't be deleted by the host
        // user, so empty the shared directories from inside before removing the container.
        if cfg!(target_os = "linux") {
            let mut targets = vec![RUNNER];
            if !self.bind {
                targets.push(WORKSPACE);
            }
            let mut cmd = Command::new(&self.binary);
            cmd.args(["exec", &self.name, "find"]).args(&targets).args([
                "-mindepth",
                "1",
                "-delete",
            ]);
            let _ = util::capture(cmd, Duration::from_secs(120));
        }
        self.remove();
        log(format!("Removed container {}", self.name));
        if let Some(dir) = self.dir.take() {
            let _ = dir.close();
        }
    }
}

impl Drop for ContainerSession {
    fn drop(&mut self) {
        self.remove();
    }
}
