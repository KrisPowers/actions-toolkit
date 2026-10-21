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

impl WorkflowFile {
    pub fn load(path: PathBuf) -> Self {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_yaml::from_str::<Workflow>(&text).map_err(|e| e.to_string()))
            .map(Arc::new);
        Self {
            path,
            file_name,
            parsed,
        }
    }

    pub fn workflow(&self) -> Option<&Arc<Workflow>> {
        self.parsed.as_ref().ok()
    }

    pub fn display_name(&self) -> String {
        self.workflow()
            .and_then(|w| w.name.clone())
            .unwrap_or_else(|| self.file_name.clone())
    }

    pub fn events(&self) -> Vec<String> {
        self.workflow().map(|w| w.events()).unwrap_or_default()
    }

    fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        let stem = self
            .file_name
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(&self.file_name);
        self.file_name.to_lowercase() == q
            || stem.to_lowercase() == q
            || self.display_name().to_lowercase() == q
            || self.path.ends_with(query)
    }
}

/// Walks up from `start` to the directory that holds `.github/workflows` (or `.git`).
pub fn find_root(start: &Path) -> PathBuf {
    for dir in start.ancestors() {
        if dir.join(".github").join("workflows").is_dir() {
            return dir.to_path_buf();
        }
    }
    for dir in start.ancestors() {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
    }
    start.to_path_buf()
}

pub fn workflows_dir(root: &Path) -> PathBuf {
    root.join(".github").join("workflows")
}

/// Loads every `.yml`/`.yaml` file under `.github/workflows`, sorted by file name.
pub fn discover(root: &Path) -> Vec<WorkflowFile> {
    let Ok(entries) = fs::read_dir(workflows_dir(root)) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|ext| ext == "yml" || ext == "yaml")
        })
        .collect();
    paths.sort();
    paths.into_iter().map(WorkflowFile::load).collect()
}
