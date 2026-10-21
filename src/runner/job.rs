//! Executes a single job: provisions a session, runs each step, collects outputs, tears down.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use super::{resolve_image, LogKind, RunEvent, RunOptions, Status};
use crate::engine::{Engine, ExecRequest, Session, SessionRequest};
use crate::util::{self, Cancel};
use crate::workflow::action::{self, ActionRef};
use crate::workflow::expr::{self, Ctx, JobState};
use crate::workflow::model::{ActionDef, Step};
use crate::workflow::plan::{step_title, PlannedJob, RunPlan};

pub(crate) struct JobContext {
    pub index: usize,
    pub plan: Arc<RunPlan>,
    pub opts: Arc<RunOptions>,
    pub engine: Arc<dyn Engine>,
    pub tx: Sender<RunEvent>,
    pub cancel: Cancel,
    pub needs: Value,
}

pub(crate) struct JobResult {
    pub status: Status,
    pub outputs: Map<String, Value>,
}

/// Sends events for one job, masking secrets in every log line.
struct Emitter {
    tx: Sender<RunEvent>,
    job: usize,
    masks: RefCell<Vec<String>>,
}

impl Emitter {
    fn log(&self, step: usize, kind: LogKind, text: impl Into<String>) {
        let mut text = text.into();
        for m in self.masks.borrow().iter() {
            if text.contains(m.as_str()) {
                text = text.replace(m.as_str(), "***");
            }
        }
        let _ = self.tx.send(RunEvent::Log {
            job: self.job,
            step,
            kind,
            text,
        });
    }

    fn add_mask(&self, value: &str) {
        let v = value.trim();
        if !v.is_empty() {
            self.masks.borrow_mut().push(v.to_string());
        }
    }

    fn step(&self, step: usize, status: Status) {
        let _ = self.tx.send(RunEvent::Step {
            job: self.job,
            step,
            status,
        });
    }

    fn job(&self, status: Status) {
        let _ = self.tx.send(RunEvent::Job {
            job: self.job,
            status,
        });
    }
}

pub(crate) fn run(cx: JobContext) -> JobResult {
    let pj = &cx.plan.jobs[cx.index];
    let em = Emitter {
        tx: cx.tx.clone(),
        job: cx.index,
        masks: RefCell::new(
            cx.opts
                .secrets
                .values()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .collect(),
        ),
    };
    let last = pj.step_names.len() - 1;
    em.job(Status::Running);
    em.step(0, Status::Running);

    let session = match setup(&cx, pj, &em) {
        Ok(s) => s,
        Err(e) => {
            let cancelled = cx.cancel.is_cancelled();
            if !cancelled {
                em.log(0, LogKind::Error, format!("{e:#}"));
            }
            let status = if cancelled {
                Status::Cancelled
            } else {
                Status::Failure
            };
            em.step(0, status);
            for i in 1..=last {
                em.step(i, Status::Skipped);
            }
            em.job(status);
            return JobResult {
                status,
                outputs: Map::new(),
            };
        }
    };
    em.step(0, Status::Success);

    let mut job = JobRun::new(&cx, pj, em, session);
    let status = job.execute();
    let outputs = job.outputs();

    job.em.step(last, Status::Running);
    {
        let JobRun { session, em, .. } = &mut job;
        session.teardown(&mut |l| em.log(last, LogKind::System, l));
    }
    job.em.step(last, Status::Success);
    job.em.job(status);
    JobResult { status, outputs }
}

fn setup(cx: &JobContext, pj: &PlannedJob, em: &Emitter) -> Result<Box<dyn Session>> {
    if let Some(uses) = &pj.job.uses {
        bail!("reusable workflows (`uses: {uses}`) are not supported yet");
    }
    if pj.job.services.is_some() {
        em.log(
            0,
            LogKind::Warning,
            "Service containers (`services:`) are not supported yet; skipping them",
        );
    }
    let engine = &cx.engine;
    em.log(
        0,
        LogKind::System,
        format!("Engine: {}", engine.kind().name()),
    );
    if !pj.runs_on.is_empty() {
        em.log(
            0,
            LogKind::System,
            format!("runs-on: {}", pj.runs_on.join(", ")),
        );
    }
    let data = json!({ "matrix": pj.matrix, "github": { "event_name": cx.opts.event } });
    let ctx = Ctx::new(&data, JobState::default());
    let spec = pj.job.container.as_ref().map(|c| c.spec());

    let mut req = SessionRequest {
        id: format!("{}-{}-{}", cx.opts.run_id, cx.index, util::slug(&pj.label)),
        source: &cx.opts.root,
        image: None,
        container_env: Vec::new(),
        volumes: Vec::new(),
        options: None,
        bind: cx.opts.bind,
        pull: cx.opts.pull,
        cancel: cx.cancel.clone(),
    };
    if engine.uses_images() {
        let image = match &spec {
            Some(s) => expr::interpolate(&s.image, &ctx).map_err(|e| anyhow!(e))?,
            None => resolve_image(&cx.opts, &pj.runs_on, &mut |l| {
                em.log(0, LogKind::Warning, l)
            })?,
        };
        em.log(0, LogKind::System, format!("Image: {image}"));
        req.image = Some(image);
        if let Some(s) = &spec {
            for (k, v) in &s.env {
                req.container_env.push((
                    k.clone(),
                    expr::interpolate_yaml(v, &ctx).map_err(|e| anyhow!(e))?,
                ));
            }
            req.volumes = s.volumes.clone();
            req.options = s.options.clone();
        }
    } else if spec.is_some() {
        em.log(
            0,
            LogKind::Warning,
            format!(
                "`container:` is ignored by the {} engine",
                engine.kind().name()
            ),
        );
    }
    let started = Instant::now();
    let session = engine.start(&req, &mut |l| em.log(0, LogKind::System, l))?;
    em.log(
        0,
        LogKind::System,
        format!(
            "Environment ready in {}",
            util::fmt_duration(started.elapsed())
        ),
    );
    Ok(session)
}

/// Where a step's commands come from: the job itself, or a composite action.
#[derive(Clone, Default)]
struct Scope {
    nested: bool,
    inputs: Option<Value>,
    action_path: Option<String>,
    env: IndexMap<String, String>,
}

struct StepFiles {
    env: String,
    output: String,
    path: String,
    summary: String,
    state: String,
}

struct JobRun<'a> {
    cx: &'a JobContext,
    pj: &'a PlannedJob,
    em: Emitter,
    session: Box<dyn Session>,
    /// Workflow + job env, plus anything appended to `$GITHUB_ENV`.
    env: IndexMap<String, String>,
    /// Entries appended to `$GITHUB_PATH`, most recent first.
    paths: Vec<String>,
    steps: Map<String, Value>,
    failed: bool,
    github: Value,
    runner: Value,
    secrets: Value,
    vars: Value,
    inputs: Value,
    event_path: String,
    copied_actions: HashSet<String>,
    seq: usize,
    deadline: Option<Instant>,
}
