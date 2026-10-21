//! Turns workflows into a concrete list of jobs: matrix expanded, dependencies resolved.

use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::expr::{self, Ctx, JobState};
use super::model::{Job, Step, Workflow};
