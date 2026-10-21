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
