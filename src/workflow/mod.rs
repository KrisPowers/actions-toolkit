//! Workflow discovery, parsing and planning.

pub mod action;
pub mod expr;
pub mod model;
pub mod plan;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};

pub use model::Workflow;

/// A workflow file found on disk, parsed or not.
pub struct WorkflowFile {
    pub path: PathBuf,
    pub file_name: String,
    pub parsed: Result<Arc<Workflow>, String>,
}
