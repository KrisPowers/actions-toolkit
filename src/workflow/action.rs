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

/// Downloads (or reuses from cache) a GitHub-hosted action, returning its local directory.
pub fn fetch(
    owner: &str,
    repo: &str,
    git_ref: &str,
    refresh: bool,
    log: &mut dyn FnMut(String),
) -> Result<PathBuf> {
    let cache = dirs::cache_dir()
        .context("could not determine a cache directory")?
        .join("maverick")
        .join("actions");
    let dir = cache.join(owner).join(repo).join(util::slug(git_ref));
    let marker = dir.join(".maverick-complete");
    let _guard = FETCH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if marker.exists() && !refresh {
        return Ok(dir);
    }
    let tmp = dir.with_extension(format!("tmp{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.parent().unwrap_or(&cache))?;
    let url = format!("https://github.com/{owner}/{repo}");
    log(format!("Downloading action {owner}/{repo}@{git_ref}"));

    let git = |args: &[&str]| -> Result<util::Captured> {
        let mut cmd = Command::new("git");
        cmd.args(args).env("GIT_TERMINAL_PROMPT", "0");
        util::capture(cmd, Duration::from_secs(180))
    };
    let tmp_s = tmp.to_string_lossy().into_owned();
    let shallow = git(&[
        "clone", "--quiet", "--depth", "1", "--branch", git_ref, &url, &tmp_s,
    ])?;
    if !shallow.success {
        // `--branch` only accepts branches and tags; fall back to a full clone for commit SHAs.
        let _ = fs::remove_dir_all(&tmp);
        let full = git(&["clone", "--quiet", &url, &tmp_s])?;
        if !full.success {
            bail!("could not clone {url}: {}", full.stderr.trim());
        }
        let co = git(&["-C", &tmp_s, "checkout", "--quiet", git_ref])?;
        if !co.success {
            bail!(
                "{owner}/{repo} has no ref `{git_ref}`: {}",
                co.stderr.trim()
            );
        }
    }
    let _ = fs::remove_dir_all(tmp.join(".git"));
    fs::write(tmp.join(".maverick-complete"), git_ref)?;
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&tmp, &dir)?;
    Ok(dir)
}

/// Reads `action.yml` (or `action.yaml`) from `dir`.
pub fn load(dir: &Path) -> Result<ActionDef> {
    for name in ["action.yml", "action.yaml"] {
        let path = dir.join(name);
        if path.exists() {
            let text = fs::read_to_string(&path)?;
            return serde_yaml::from_str(&text)
                .with_context(|| format!("invalid {}", path.display()));
        }
    }
    bail!("no action.yml found in {}", dir.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_refs() {
        assert_eq!(parse("actions/checkout@v4").unwrap(), ActionRef::Checkout);
        assert_eq!(
            parse("./.github/actions/setup").unwrap(),
            ActionRef::Local("./.github/actions/setup".into())
        );
        assert_eq!(
            parse("docker://alpine:3").unwrap(),
            ActionRef::Docker("alpine:3".into())
        );
        assert_eq!(
            parse("owner/repo/sub/dir@v1").unwrap(),
            ActionRef::Remote {
                owner: "owner".into(),
                repo: "repo".into(),
                path: Some("sub/dir".into()),
                git_ref: "v1".into()
            }
        );
        assert!(parse("owner/repo").is_err());
    }
}
