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
