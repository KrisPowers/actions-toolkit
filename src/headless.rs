//! Runs without the TUI: in the foreground for `atk run`, or as a detached background
//! process for `atk run --detach` and the AI tools. Either way the run is recorded in the
//! history as it goes, so it can be watched, reviewed and cancelled by id.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::engine;
use crate::history::{self, Outcome, Recorder, RunMeta, Store};
use crate::runner::{self, RunEvent, RunOptions};
use crate::util::Cancel;
use crate::workflow::{self, plan::RunPlan};

/// What to run, and the overrides on top of the project's config.
#[derive(Clone, Debug, Default)]
pub struct RunRequest {
    pub workflow: PathBuf,
    pub job: Option<String>,
    pub event: Option<String>,
    pub inputs: BTreeMap<String, String>,
    pub vars: BTreeMap<String, String>,
    /// `runs-on` label -> image, on top of the configured ones.
    pub images: BTreeMap<String, String>,
    pub pull: bool,
    pub bind: bool,
}

impl RunRequest {
    pub fn event(&self, cfg: &Config) -> String {
        self.event
            .clone()
            .or_else(|| cfg.event.clone())
            .unwrap_or_else(|| "push".into())
    }

    fn plan(&self) -> Result<RunPlan> {
        workflow::plan_for(&self.workflow, self.job.as_deref()).map_err(anyhow::Error::msg)
    }

    /// The `atk run` arguments that reproduce this request.
    fn args(&self) -> Vec<String> {
        let mut args = vec!["run".to_string(), self.workflow.display().to_string()];
        let mut opt = |flag: &str, value: String| {
            args.push(flag.to_string());
            args.push(value);
        };
        if let Some(job) = &self.job {
            opt("--job", job.clone());
        }
        if let Some(event) = &self.event {
            opt("--event", event.clone());
        }
        for (k, v) in &self.inputs {
            opt("--input", format!("{k}={v}"));
        }
        for (k, v) in &self.vars {
            opt("--var", format!("{k}={v}"));
        }
        for (k, v) in &self.images {
            opt("--image", format!("{k}={v}"));
        }
        if self.pull {
            args.push("--pull".into());
        }
        if self.bind {
            args.push("--bind".into());
        }
        args
    }
}

/// Runs `req` to the end in this process, calling `on_event` for every runner event.
/// Ctrl-C and `atk cancel <id>` both stop it; the run is recorded under `id` either way.
pub fn execute(
    root: &Path,
    store: Option<Store>,
    req: &RunRequest,
    id: String,
    on_event: &mut dyn FnMut(&RunPlan, &RunEvent),
) -> Result<Outcome> {
    let cfg = Config::load(root)?;
    let plan = Arc::new(req.plan()?);
    let engine = engine::build()?;
    let event = req.event(&cfg);
    let mut opts = RunOptions::new(root, &cfg, &event);
    opts.inputs.extend(req.inputs.clone());
    opts.vars.extend(req.vars.clone());
    opts.platforms.extend(req.images.clone());
    opts.pull = req.pull;
    opts.bind = req.bind;

    let mut recorder = store.map(|s| {
        let meta = RunMeta {
            project: root.to_path_buf(),
            workflow: req.workflow.clone(),
            job: req.job.clone(),
            event,
            engine: engine.name().to_string(),
        };
        Recorder::begin(s, id, &plan, meta)
    });
    let cancel = Cancel::new();
    let on_ctrl_c = cancel.clone();
    // Only one handler can exist per process; a second run in it keeps the first one's.
    let _ = ctrlc::set_handler(move || on_ctrl_c.cancel());

    let (rx, handle) = runner::spawn(plan.clone(), engine, Arc::new(opts), cancel.clone());
    let success = loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(event) => {
                if let Some(rec) = &mut recorder {
                    rec.apply(&event);
                }
                on_event(&plan, &event);
                if let RunEvent::Finished { success } = event {
                    break success;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break false,
        }
        if recorder.as_mut().is_some_and(Recorder::tick) {
            cancel.cancel();
        }
    };
    let _ = handle.join();
    let outcome = if success {
        Outcome::Success
    } else if cancel.is_cancelled() {
        Outcome::Cancelled
    } else {
        Outcome::Failure
    };
    if let Some(rec) = &mut recorder {
        rec.finish(outcome);
        if let Some(err) = rec.error() {
            eprintln!("warning: {err}");
        }
    }
    Ok(outcome)
}

/// Starts `req` as a background `atk run` that outlives this process; returns its run id
/// once the run shows up in the history.
pub fn spawn_detached(root: &Path, store: &Store, req: &RunRequest) -> Result<String> {
    // Fail here, with a useful message, rather than silently in the background.
    req.plan()?;
    engine::build()?;
    let id = history::new_id();
    let exe = std::env::current_exe().context("cannot find the atk executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("-C")
        .arg(root)
        .args(req.args())
        .args(["--run-id", id.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut cmd);
    let mut child = cmd.spawn().context("could not start the background run")?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if store.get(&id).is_some() {
            return Ok(id);
        }
        if let Some(status) = child.try_wait()? {
            if store.get(&id).is_some() {
                return Ok(id);
            }
            bail!("the background run exited before starting ({status})");
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("the background run did not start within 20s")
}

#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // Its own process group, so Ctrl-C in this terminal doesn't reach it.
    cmd.process_group(0);
}

#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    // A hidden console of its own, which the docker processes it starts share.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

/// Waits until run `id` ends or `timeout` passes; returns whether it ended.
pub fn wait(store: &Store, id: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if store.get(id).is_none_or(|r| r.outcome.is_done()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_to_arguments() {
        let req = RunRequest {
            workflow: PathBuf::from("ci.yml"),
            job: Some("test".into()),
            event: Some("pull_request".into()),
            inputs: [("greeting".to_string(), "hi there".to_string())].into(),
            pull: true,
            ..Default::default()
        };
        assert_eq!(
            req.args(),
            [
                "run",
                "ci.yml",
                "--job",
                "test",
                "--event",
                "pull_request",
                "--input",
                "greeting=hi there",
                "--pull"
            ]
        );
    }
}
