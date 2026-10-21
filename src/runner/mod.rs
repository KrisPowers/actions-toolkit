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
