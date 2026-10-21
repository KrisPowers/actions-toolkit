//! Schedules planned jobs onto an engine, honouring `needs`, `if`, fail-fast and parallelism.

mod events;
mod job;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};

pub use events::{LogKind, RunEvent, Status};

use crate::cli::RunArgs;
use crate::config::Config;
use crate::engine::Engine;
use crate::util::{self, Cancel};
use crate::workflow::expr::{self, Ctx, JobState};
use crate::workflow::plan::RunPlan;

pub const DEFAULT_IMAGE: &str = "ghcr.io/catthehacker/ubuntu:act-latest";

/// Everything about a run that isn't the plan or the engine.
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub root: PathBuf,
    pub event: String,
    pub event_payload: Option<Value>,
    pub secrets: BTreeMap<String, String>,
    pub vars: BTreeMap<String, String>,
    pub inputs: BTreeMap<String, String>,
    /// `runs-on` label -> container image.
    pub platforms: BTreeMap<String, String>,
    pub bind: bool,
    pub pull: bool,
    pub parallel: usize,
    pub git: GitInfo,
    pub run_id: String,
}

impl RunOptions {
    pub fn new(root: &Path, cfg: &Config, event: &str) -> Self {
        let mut platforms: BTreeMap<String, String> = [
            ("ubuntu-latest", DEFAULT_IMAGE),
            ("ubuntu-24.04", "ghcr.io/catthehacker/ubuntu:act-24.04"),
            ("ubuntu-22.04", "ghcr.io/catthehacker/ubuntu:act-22.04"),
            ("ubuntu-20.04", "ghcr.io/catthehacker/ubuntu:act-20.04"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        platforms.extend(cfg.images.clone());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Self {
            root: root.to_path_buf(),
            event: event.to_string(),
            event_payload: None,
            secrets: BTreeMap::new(),
            vars: BTreeMap::new(),
            inputs: BTreeMap::new(),
            platforms,
            bind: false,
            pull: false,
            parallel: cfg
                .parallel
                .unwrap_or_else(|| {
                    thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(4)
                })
                .max(1),
            git: GitInfo::detect(root),
            run_id: (now.as_secs() % 1_000_000_000).to_string(),
        }
    }

    pub fn apply_args(&mut self, args: &RunArgs) -> Result<()> {
        self.event = args.event.clone();
        self.bind = args.bind;
        self.pull = args.pull;
        if let Some(n) = args.parallel {
            self.parallel = n.max(1);
        }
        for p in &args.platforms {
            let (label, image) = p
                .split_once('=')
                .with_context(|| format!("--platform expects LABEL=IMAGE, got `{p}`"))?;
            self.platforms.insert(label.to_string(), image.to_string());
        }
        if let Some(file) = &args.secret_file {
            let text =
                fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
            self.secrets.extend(parse_dotenv(&text));
        }
        for s in &args.secrets {
            match s.split_once('=') {
                Some((k, v)) => self.secrets.insert(k.to_string(), v.to_string()),
                // `-s NAME` reads the value from the environment, keeping it out of shell history.
                None => self.secrets.insert(
                    s.clone(),
                    std::env::var(s)
                        .with_context(|| format!("secret {s} has no value and ${s} is not set"))?,
                ),
            };
        }
        for (target, list, flag) in [
            (&mut self.vars, &args.vars, "--var"),
            (&mut self.inputs, &args.inputs, "--input"),
        ] {
            for item in list {
                let (k, v) = item
                    .split_once('=')
                    .with_context(|| format!("{flag} expects KEY=VALUE, got `{item}`"))?;
                target.insert(k.to_string(), v.to_string());
            }
        }
        if let Some(file) = &args.event_file {
            let text =
                fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
            self.event_payload =
                Some(serde_json::from_str(&text).context("--event-file must contain JSON")?);
        }
        Ok(())
    }
}

fn parse_dotenv(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("export ").unwrap_or(l).split_once('='))
        .map(|(k, v)| {
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(v);
            (k.trim().to_string(), v.to_string())
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct GitInfo {
    pub sha: String,
    pub branch: String,
    pub repository: String,
    pub owner: String,
    pub actor: String,
}

impl GitInfo {
    pub fn detect(root: &Path) -> Self {
        let git = |args: &[&str]| -> String {
            let mut cmd = Command::new("git");
            cmd.arg("-C").arg(root).args(args);
            util::capture(cmd, Duration::from_secs(3))
                .ok()
                .filter(|o| o.success)
                .map(|o| o.stdout.trim().to_string())
                .unwrap_or_default()
        };
        let remote = git(&["config", "--get", "remote.origin.url"]);
        let repository = parse_repo(&remote).unwrap_or_else(|| {
            let dir = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("local/{dir}")
        });
        let owner = repository.split('/').next().unwrap_or_default().to_string();
        let actor = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| owner.clone());
        let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]);
        Self {
            sha: git(&["rev-parse", "HEAD"]),
            branch: if branch.is_empty() {
                "main".into()
            } else {
                branch
            },
            repository,
            owner,
            actor,
        }
    }

    pub fn git_ref(&self) -> String {
        format!("refs/heads/{}", self.branch)
    }
}

fn parse_repo(url: &str) -> Option<String> {
    let path = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let path = path.rsplit_once(':').map(|(_, p)| p).unwrap_or(path);
    let mut parts = path.rsplit('/');
    let repo = parts.next().filter(|s| !s.is_empty())?;
    let owner = parts.next().filter(|s| !s.is_empty() && !s.contains('.'))?;
    Some(format!("{owner}/{repo}"))
}

/// Starts executing `plan` on a background thread; events arrive on the returned receiver.
pub fn spawn(
    plan: Arc<RunPlan>,
    engine: Arc<dyn Engine>,
    opts: Arc<RunOptions>,
    cancel: Cancel,
) -> (Receiver<RunEvent>, JoinHandle<bool>) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let ok = schedule(&plan, &engine, &opts, &tx, &cancel);
        let _ = tx.send(RunEvent::Finished { success: ok });
        ok
    });
    (rx, handle)
}

fn schedule(
    plan: &Arc<RunPlan>,
    engine: &Arc<dyn Engine>,
    opts: &Arc<RunOptions>,
    tx: &Sender<RunEvent>,
    cancel: &Cancel,
) -> bool {
    let n = plan.jobs.len();
    let mut results: Vec<Option<job::JobResult>> = (0..n).map(|_| None).collect();
    let mut started = vec![false; n];
    let mut tokens: Vec<Cancel> = (0..n).map(|_| Cancel::new()).collect();
    let (done_tx, done_rx) = mpsc::channel::<(usize, job::JobResult)>();
    let mut running = 0;

    let finish = |i: usize, status: Status, results: &mut Vec<Option<job::JobResult>>| {
        let _ = tx.send(RunEvent::Job { job: i, status });
        for step in 0..plan.jobs[i].step_names.len() {
            let _ = tx.send(RunEvent::Step {
                job: i,
                step,
                status: Status::Skipped,
            });
        }
        results[i] = Some(job::JobResult {
            status,
            outputs: Map::new(),
        });
    };

    loop {
        // Start (or skip) every job whose dependencies are finished, until nothing changes.
        let mut progressed = true;
        while progressed {
            progressed = false;
            for i in 0..n {
                if started[i] || results[i].is_some() {
                    continue;
                }
                let pj = &plan.jobs[i];
                if pj.needs.iter().any(|&d| results[d].is_none()) {
                    continue;
                }
                let needs = needs_context(plan, &results, i);
                let state = JobState {
                    failed: pj.needs.iter().any(|&d| {
                        matches!(results[d].as_ref().map(|r| r.status), Some(Status::Failure))
                    }),
                    blocked: pj.needs.iter().any(|&d| {
                        !matches!(results[d].as_ref().map(|r| r.status), Some(Status::Success))
                    }),
                    cancelled: cancel.is_cancelled(),
                };
                let data = json!({
                    "github": { "event_name": opts.event, "ref": opts.git.git_ref(), "sha": opts.git.sha, "repository": opts.git.repository },
                    "needs": needs,
                    "matrix": pj.matrix,
                    "vars": expr::object(&opts.vars),
                    "inputs": expr::object(&opts.inputs),
                });
                let should_run =
                    match expr::condition(pj.job.condition.as_ref(), &Ctx::new(&data, state)) {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = tx.send(RunEvent::Log {
                                job: i,
                                step: 0,
                                kind: LogKind::Error,
                                text: format!("invalid job `if`: {e}"),
                            });
                            finish(i, Status::Failure, &mut results);
                            progressed = true;
                            continue;
                        }
                    };
                if !should_run {
                    let status = if cancel.is_cancelled() {
                        Status::Cancelled
                    } else {
                        Status::Skipped
                    };
                    finish(i, status, &mut results);
                    progressed = true;
                    continue;
                }
                if running >= opts.parallel {
                    continue;
                }
                started[i] = true;
                running += 1;
                progressed = true;
                let cx = job::JobContext {
                    index: i,
                    plan: plan.clone(),
                    opts: opts.clone(),
                    engine: engine.clone(),
                    tx: tx.clone(),
                    cancel: tokens[i].clone(),
                    needs,
                };
                let done = done_tx.clone();
                thread::spawn(move || {
                    let result = job::run(cx);
                    let _ = done.send((i, result));
                });
            }
        }

        if results.iter().all(Option::is_some) {
            break;
        }
        if running == 0 {
            // Nothing running and nothing startable: should be impossible for an acyclic plan.
            break;
        }
        match done_rx.recv_timeout(Duration::from_millis(100)) {
            Ok((i, result)) => {
                running -= 1;
                let failed = result.status == Status::Failure;
                results[i] = Some(result);
                if failed && plan.jobs[i].fail_fast && plan.jobs[i].matrix_total > 1 {
                    for j in 0..n {
                        let same = Arc::ptr_eq(&plan.jobs[j].workflow, &plan.jobs[i].workflow)
                            && plan.jobs[j].key == plan.jobs[i].key;
                        if same && j != i && results[j].is_none() {
                            if started[j] {
                                tokens[j].cancel();
                            } else {
                                finish(j, Status::Cancelled, &mut results);
                            }
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if cancel.is_cancelled() {
            for t in tokens.iter_mut() {
                t.cancel();
            }
        }
    }

    results.iter().all(|r| {
        matches!(
            r.as_ref().map(|r| r.status),
            Some(Status::Success | Status::Skipped)
        )
    })
}
