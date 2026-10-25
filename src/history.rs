//! Past runs, kept as JSON files in the user data folder: `<id>.json` holds the summary and
//! `<id>.logs.json` the log lines, so listing runs never reads logs.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::runner::{LogKind, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Success,
    Failure,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub name: String,
    pub status: Status,
    pub millis: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub name: String,
    pub status: Status,
    pub millis: Option<u64>,
    pub steps: Vec<StepRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    /// Project root the run belongs to.
    pub project: PathBuf,
    pub title: String,
    /// Workflow file that was run, and the single job if only one was picked.
    pub workflow: PathBuf,
    pub job: Option<String>,
    pub engine: String,
    pub event: String,
    /// Unix time in seconds.
    pub started: u64,
    pub millis: u64,
    pub outcome: Outcome,
    pub jobs: Vec<JobRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub job: usize,
    pub step: usize,
    pub kind: LogKind,
    pub text: String,
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// A unique, time-ordered run id.
pub fn new_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    format!(
        "{millis}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// "just now", "5m ago", "3h ago", "2d ago".
pub fn ago(started: u64) -> String {
    let secs = now().saturating_sub(started);
    match secs {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86_399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The store in the user data folder (`<data dir>/maverick/runs`).
    pub fn open_default() -> Option<Store> {
        dirs::data_dir().map(|d| Store::at(d.join("maverick").join("runs")))
    }

    pub fn at(dir: PathBuf) -> Store {
        Store { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn summary_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn logs_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.logs.json"))
    }

    pub fn save(&self, record: &RunRecord, logs: &[LogLine]) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        write_atomic(&self.logs_path(&record.id), &serde_json::to_vec(logs)?)?;
        // The summary goes last: a run only shows up once both files exist.
        write_atomic(
            &self.summary_path(&record.id),
            &serde_json::to_vec_pretty(record)?,
        )
    }

    /// Runs for `project`, newest first. Unreadable files are skipped.
    pub fn list(&self, project: &Path) -> Vec<RunRecord> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut runs: Vec<RunRecord> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".json") && !n.ends_with(".logs.json"))
            })
            .filter_map(|p| serde_json::from_slice::<RunRecord>(&fs::read(p).ok()?).ok())
            .filter(|r| r.project == project)
            .collect();
        runs.sort_by(|a, b| b.started.cmp(&a.started).then_with(|| b.id.cmp(&a.id)));
        runs
    }

    pub fn logs(&self, id: &str) -> Vec<LogLine> {
        fs::read(self.logs_path(id))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        for path in [self.summary_path(id), self.logs_path(id)] {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("deleting {}", path.display()))
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Deletes every run of `project`; returns how many were removed.
    pub fn clear(&self, project: &Path) -> Result<usize> {
        let runs = self.list(project);
        for run in &runs {
            self.delete(&run.id)?;
        }
        Ok(runs.len())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(project: &Path, started: u64) -> RunRecord {
        RunRecord {
            id: new_id(),
            project: project.to_path_buf(),
            title: "CI".into(),
            workflow: project.join("ci.yml"),
            job: None,
            engine: "Maverick Native".into(),
            event: "push".into(),
            started,
            millis: 1200,
            outcome: Outcome::Success,
            jobs: vec![JobRecord {
                name: "build".into(),
                status: Status::Success,
                millis: Some(1100),
                steps: vec![],
            }],
        }
    }

    #[test]
    fn saves_lists_and_deletes_runs() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("runs"));
        let project = Path::new("/projects/app");
        let older = record(project, 100);
        let newer = record(project, 200);
        let line = LogLine {
            job: 0,
            step: 1,
            kind: LogKind::Output,
            text: "hello".into(),
        };
        store.save(&older, &[line]).unwrap();
        store.save(&newer, &[]).unwrap();
        store
            .save(&record(Path::new("/projects/other"), 300), &[])
            .unwrap();

        let runs = store.list(project);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].id, newer.id, "newest first");
        assert_eq!(store.logs(&older.id)[0].text, "hello");

        store.delete(&older.id).unwrap();
        assert_eq!(store.list(project).len(), 1);
        assert!(store.logs(&older.id).is_empty());
        assert_eq!(store.clear(project).unwrap(), 1);
        assert!(store.list(project).is_empty());
        assert_eq!(store.list(Path::new("/projects/other")).len(), 1);
    }
}
