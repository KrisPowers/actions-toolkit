//! Resolving `uses:` references to runnable actions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::model::ActionDef;
use crate::util;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionRef {
    /// `actions/checkout`: the workspace is already provisioned by maverick.
    Checkout,
    /// A well-known action that has no local equivalent yet; skipped with an explanation.
    Unsupported(&'static str),
    Docker(String),
    Local(String),
    Remote {
        owner: String,
        repo: String,
        path: Option<String>,
        git_ref: String,
    },
}
