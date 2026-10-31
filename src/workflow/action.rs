//! Resolving `uses:` references to runnable actions.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::model::ActionDef;

#[derive(Debug, Clone, PartialEq)]
pub enum ActionRef {
    /// `actions/checkout`: the workspace is already provisioned by ATK.
    Checkout,
    /// `actions/cache`, or its `restore` or `save` half, backed by a local store.
    Cache(CacheMode),
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CacheMode {
    /// Restore now, save when the job succeeds.
    Both,
    Restore,
    Save,
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
        "actions/cache" => ActionRef::Cache(match path.as_deref() {
            None => CacheMode::Both,
            Some("restore") => CacheMode::Restore,
            Some("save") => CacheMode::Save,
            Some(_) => bail!("`{u}` is not part of actions/cache"),
        }),
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

/// Finds a local copy of an `owner/repo@ref` action. ATK never downloads anything, so
/// these must be placed by hand under `.atk/actions/` in the project or the user config
/// directory, either as `owner/repo@ref` or `owner/repo`.
pub fn locate(root: &Path, owner: &str, repo: &str, git_ref: &str) -> Option<PathBuf> {
    let mut bases = vec![root.join(".atk").join("actions")];
    if let Some(dir) = dirs::config_dir() {
        bases.push(dir.join("atk").join("actions"));
    }
    bases.iter().find_map(|base| {
        let owner_dir = base.join(owner);
        [
            owner_dir.join(format!("{repo}@{git_ref}")),
            owner_dir.join(repo),
        ]
        .into_iter()
        .find(|d| d.is_dir())
    })
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
        assert_eq!(
            parse("actions/cache@v4").unwrap(),
            ActionRef::Cache(CacheMode::Both)
        );
        assert_eq!(
            parse("actions/cache/save@v4").unwrap(),
            ActionRef::Cache(CacheMode::Save)
        );
        assert!(parse("owner/repo").is_err());
    }
}
