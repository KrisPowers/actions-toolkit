//! User (`<config dir>/atk/config.toml`) and project (`.atk.toml`) configuration.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Event runs simulate unless another one is picked (`push`, `pull_request`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Maximum number of jobs to run at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel: Option<usize>,
    /// `runs-on` label -> container image.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, String>,
    /// Extra places to look for workflows: folders or single files. Relative paths are
    /// resolved against the project root (project config) or the home directory (user config).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<String>,
    /// Values available to workflows as `vars.NAME`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
}

impl Config {
    pub fn user_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("atk").join("config.toml"))
    }

    pub fn project_path(root: &Path) -> PathBuf {
        root.join(".atk.toml")
    }

    /// Loads the user config, then overlays the project config on top of it. Workflow
    /// locations are not merged; see `workflow::locations`.
    pub fn load(root: &Path) -> Result<Config> {
        let mut cfg = Self::load_user()?;
        let p = Self::load_project(root)?;
        if p.parallel.is_some() {
            cfg.parallel = p.parallel;
        }
        if p.event.is_some() {
            cfg.event = p.event;
        }
        cfg.images.extend(p.images);
        cfg.vars.extend(p.vars);
        Ok(cfg)
    }

    pub fn load_user() -> Result<Config> {
        match Self::user_path() {
            Some(path) if path.exists() => read(&path),
            _ => Ok(Config::default()),
        }
    }

    pub fn load_project(root: &Path) -> Result<Config> {
        let path = Self::project_path(root);
        if path.exists() {
            read(&path)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save_user(&self) -> Result<PathBuf> {
        let path = Self::user_path().context("could not determine a config directory")?;
        write(&path, self)
    }

    pub fn save_project(&self, root: &Path) -> Result<PathBuf> {
        write(&Self::project_path(root), self)
    }
}

fn write(path: &Path, cfg: &Config) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(cfg)?)?;
    Ok(path.to_path_buf())
}

fn read(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path)?;
    toml::from_str(&text).with_context(|| format!("invalid config file {}", path.display()))
}
