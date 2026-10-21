//! Workflow discovery, parsing and planning.

pub mod action;
pub mod expr;
pub mod model;
pub mod plan;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::Config;
pub use model::Workflow;

/// Where a workflow location came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A conventional folder maverick always checks.
    Auto,
    /// Listed in the project's `.maverick.toml`.
    Project,
    /// Listed in the user config; applies to every project.
    Global,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Auto => "auto",
            Source::Project => "project",
            Source::Global => "global",
        }
    }
}

/// Folders searched automatically, relative to the project root.
pub const AUTO_DIRS: [&str; 2] = [".github/workflows", ".maverick/workflows"];

/// A folder or single file to search for workflows.
#[derive(Debug, Clone)]
pub struct Location {
    pub source: Source,
    /// As written in config (or the auto folder name).
    pub entry: String,
    pub path: PathBuf,
}

/// A workflow file found on disk, parsed or not.
pub struct WorkflowFile {
    pub path: PathBuf,
    pub file_name: String,
    pub source: Source,
    pub parsed: Result<Arc<Workflow>, String>,
}

impl WorkflowFile {
    /// Loads `path`. Files from user-chosen locations are skipped unless they look like a
    /// workflow (a YAML mapping with `jobs`), so a folder of mixed YAML stays usable.
    fn load(path: PathBuf, source: Source) -> Option<Self> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(&path);
        if source != Source::Auto {
            let looks_like_workflow = text.as_ref().ok().is_some_and(|t| {
                serde_yaml::from_str::<serde_yaml::Value>(t)
                    .ok()
                    .is_some_and(|v| v.get("jobs").is_some())
            });
            if !looks_like_workflow {
                return None;
            }
        }
        let parsed = text
            .map_err(|e| e.to_string())
            .and_then(|text| serde_yaml::from_str::<Workflow>(&text).map_err(|e| e.to_string()))
            .map(Arc::new);
        Some(Self {
            path,
            file_name,
            source,
            parsed,
        })
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
}

/// Walks up from `start` to the nearest project root: a directory with workflows or maverick
/// config, else the enclosing git checkout, else `start` itself.
pub fn find_root(start: &Path) -> PathBuf {
    let is_project = |dir: &Path| {
        AUTO_DIRS.iter().any(|d| dir.join(d).is_dir()) || Config::project_path(dir).is_file()
    };
    start
        .ancestors()
        .find(|d| is_project(d))
        .or_else(|| start.ancestors().find(|d| d.join(".git").exists()))
        .unwrap_or(start)
        .to_path_buf()
}

/// Every location searched for `root`: the auto folders, then project, then global entries.
pub fn locations(root: &Path) -> Vec<Location> {
    let mut out: Vec<Location> = AUTO_DIRS
        .iter()
        .map(|d| Location {
            source: Source::Auto,
            entry: d.to_string(),
            path: root.join(d),
        })
        .collect();
    let project = Config::load_project(root).unwrap_or_default();
    out.extend(project.workflows.iter().map(|e| Location {
        source: Source::Project,
        entry: e.clone(),
        path: resolve(e, root),
    }));
    let user = Config::load_user().unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_else(|| root.to_path_buf());
    out.extend(user.workflows.iter().map(|e| Location {
        source: Source::Global,
        entry: e.clone(),
        path: resolve(e, &home),
    }));
    out
}

/// Expands `~` and makes `entry` absolute relative to `base`.
pub fn resolve(entry: &str, base: &Path) -> PathBuf {
    let expanded = match entry.strip_prefix('~') {
        Some(rest) => match dirs::home_dir() {
            Some(home) => home.join(rest.trim_start_matches(['/', '\\'])),
            None => PathBuf::from(entry),
        },
        None => PathBuf::from(entry),
    };
    if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    }
}

fn is_yaml(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("yml") || ext.eq_ignore_ascii_case("yaml"))
}

/// The workflow files in one location (a folder's `.yml`/`.yaml` files, or a single file).
pub fn scan(loc: &Location) -> Vec<WorkflowFile> {
    let mut paths: Vec<PathBuf> = if loc.path.is_file() {
        vec![loc.path.clone()]
    } else {
        fs::read_dir(&loc.path)
            .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default()
    };
    paths.retain(|p| is_yaml(p));
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| WorkflowFile::load(p, loc.source))
        .collect()
}

/// Loads workflows from every location, skipping files already found elsewhere.
pub fn discover(root: &Path) -> Vec<WorkflowFile> {
    let mut seen = std::collections::HashSet::new();
    locations(root)
        .iter()
        .flat_map(scan)
        .filter(|f| seen.insert(f.path.canonicalize().unwrap_or_else(|_| f.path.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_locations_only_pick_up_workflows() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("ci.yml"),
            "on: push\njobs:\n  a: { steps: [] }\n",
        )
        .unwrap();
        fs::write(dir.path().join("compose.yaml"), "services: {}\n").unwrap();
        fs::write(dir.path().join("notes.txt"), "jobs: x").unwrap();
        let loc = Location {
            source: Source::Project,
            entry: "x".into(),
            path: dir.path().to_path_buf(),
        };
        let found = scan(&loc);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name, "ci.yml");
        let auto = Location {
            source: Source::Auto,
            ..loc
        };
        assert_eq!(scan(&auto).len(), 2, "auto folders show broken files too");
    }

    #[test]
    fn resolves_relative_and_home_paths() {
        let base = Path::new("/proj");
        assert_eq!(resolve("ci", base), base.join("ci"));
        if let Some(home) = dirs::home_dir() {
            assert_eq!(resolve("~/flows", base), home.join("flows"));
        }
    }
}
