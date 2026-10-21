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

pub fn parse(uses: &str) -> Result<ActionRef> {
    let u = uses.trim();
    if let Some(image) = u.strip_prefix("docker://") {
        return Ok(ActionRef::Docker(image.to_string()));
    }
    if u.starts_with("./") || u.starts_with("../") {
        return Ok(ActionRef::Local(u.to_string()));
    }
    let (target, git_ref) = u
        .rsplit_once('@')
        .with_context(|| format!("`{u}` is missing a version (`owner/repo@ref`)"))?;
    let mut parts = target.splitn(3, '/');
    let (Some(owner), Some(repo)) = (parts.next(), parts.next()) else {
        bail!("`{u}` is not a valid action reference");
    };
    let path = parts.next().map(String::from);
    Ok(match format!("{owner}/{repo}").to_lowercase().as_str() {
        "actions/checkout" => ActionRef::Checkout,
        "actions/cache" => ActionRef::Unsupported(
            "caching is skipped locally; every run starts from a fresh sandbox",
        ),
        "actions/upload-artifact" | "actions/download-artifact" => {
            ActionRef::Unsupported("artifact upload/download is not supported locally yet")
        }
        _ => ActionRef::Remote {
            owner: owner.to_string(),
            repo: repo.to_string(),
            path,
            git_ref: git_ref.to_string(),
        },
    })
}

static FETCH_LOCK: Mutex<()> = Mutex::new(());
