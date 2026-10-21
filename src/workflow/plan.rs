//! Turns workflows into a concrete list of jobs: matrix expanded, dependencies resolved.

use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::expr::{self, Ctx, JobState};
use super::model::{Job, Step, Workflow};

pub struct PlannedJob {
    pub workflow: Arc<Workflow>,
    pub workflow_name: String,
    pub key: String,
    pub label: String,
    pub job: Job,
    pub matrix: Value,
    pub runs_on: Vec<String>,
    /// Indices of jobs in the plan that must finish first.
    pub needs: Vec<usize>,
    /// UI step names: "Set up job", each step, "Complete job".
    pub step_names: Vec<String>,
    pub fail_fast: bool,
    pub matrix_index: usize,
    pub matrix_total: usize,
}
