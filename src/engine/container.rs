//! Docker: one long-lived container per job, steps run via `exec`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tempfile::TempDir;

use super::{
    apply_probe, find_binary, probe_unix, version_of, Availability, Engine, EngineInfo,
    ExecRequest, Session, SessionInfo, SessionRequest,
};
use crate::util::{self, Cancel, ExecOutcome};

const WORKSPACE: &str = "/github/workspace";
const RUNNER: &str = "/github/runner";

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
    // `info` talks to the daemon, so it tells us whether containers can actually run.
    let alive = util::probe(&bin, &["info", "--format", "{{json .}}"]).is_some_and(|o| o.success);
    EngineInfo {
        availability: if alive {
            Availability::Ready
        } else {
            Availability::NotRunning
        },
        version,
        note: (!alive).then(|| "start Docker Desktop (or the docker daemon)".into()),
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

    fn ensure_image(
        &self,
        image: &str,
        pull: bool,
        cancel: &Cancel,
        log: &mut dyn FnMut(String),
    ) -> Result<()> {
        if !pull {
            let mut inspect = self.cmd();
            inspect.args(["image", "inspect", image]);
            if util::capture(inspect, Duration::from_secs(30)).is_ok_and(|o| o.success) {
                return Ok(());
            }
        }
        log(format!("Pulling image {image}"));
        let mut cmd = self.cmd();
        cmd.args(["pull", image]);
        let outcome = util::stream(cmd, cancel, None, log)?;
        if !outcome.success() {
            bail!("failed to pull image {image}");
        }
        Ok(())
    }
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

        self.ensure_image(&image, req.pull, &req.cancel, log)?;

        let name = format!("atk-{}", req.id);
        let mut rm = self.cmd();
        rm.args(["rm", "-f", &name]);
        let _ = util::capture(rm, Duration::from_secs(30));

        let mut cmd = self.cmd();
        cmd.args([
            "run",
            "-d",
            "--name",
            &name,
            "--label",
            "dev.atk.run=1",
        ]);
        cmd.arg("--mount").arg(format!(
            "type=bind,source={},target={WORKSPACE}",
            workspace_host.display()
        ));
        cmd.arg("--mount").arg(format!(
            "type=bind,source={},target={RUNNER}",
            runner_host.display()
        ));
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
            workspace_host,
            runner_host,
            info: SessionInfo::linux(WORKSPACE, RUNNER),
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

    fn exec(
        &mut self,
        req: &ExecRequest,
        cancel: &Cancel,
        out: &mut dyn FnMut(String),
    ) -> Result<ExecOutcome> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(["exec", "-w", &req.cwd]);
        for (k, v) in &req.env {
            cmd.arg("-e").arg(format!("{k}={v}"));
        }
        cmd.arg(&self.name).args(&req.argv);
        util::stream(cmd, cancel, req.timeout, out)
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
