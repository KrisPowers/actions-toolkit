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
