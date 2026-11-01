//! Runs, kept as files in the user data folder: `<id>.json` holds the summary and
//! `<id>.logs.jsonl` the log lines, so listing runs never reads logs. Both are written while
//! a run is going, which is what lets `atk watch`, `atk status` and AI agents follow a run
//! started anywhere else: the TUI, another terminal, or a detached `atk run`.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::runner::{LogKind, RunEvent, Status};
use crate::workflow::plan::RunPlan;

/// A running run whose summary hasn't been touched for this long has lost its process.
const STALE_SECS: u64 = 30;
/// How often a live run rewrites its summary and flushes its logs.
const FLUSH_EVERY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Running,
    Success,
    Failure,
    Cancelled,
    /// Was running when its process went away without finishing the record.
    Interrupted,
}

impl Outcome {
    pub fn is_done(self) -> bool {
        self != Outcome::Running
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Running => "running",
            Outcome::Success => "success",
            Outcome::Failure => "failure",
            Outcome::Cancelled => "cancelled",
            Outcome::Interrupted => "interrupted",
        }
    }
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
    /// Unix time of the last write; a live run refreshes it constantly.
    #[serde(default)]
    pub updated: u64,
    pub millis: u64,
    pub outcome: Outcome,
    pub jobs: Vec<JobRecord>,
}

/// Files a run kept with `actions/upload-artifact`.
#[derive(Debug, Clone, Serialize)]
pub struct Artifact {
    pub name: String,
    pub files: usize,
    pub bytes: u64,
    pub path: PathBuf,
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
    /// The store in the user data folder (`<data dir>/atk/runs`).
    pub fn open_default() -> Option<Store> {
        dirs::data_dir().map(|d| Store::at(d.join("atk").join("runs")))
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
        self.dir.join(format!("{id}.logs.jsonl"))
    }

    fn cancel_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.cancel"))
    }

    /// Where run `id` keeps its artifacts, one folder per artifact name.
    pub fn artifacts_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id).join("artifacts")
    }

    /// The artifacts run `id` uploaded, by name.
    pub fn artifacts(&self, id: &str) -> Vec<Artifact> {
        let Ok(entries) = fs::read_dir(self.artifacts_dir(id)) else {
            return Vec::new();
        };
        let mut list: Vec<Artifact> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                // An upload still being copied in.
                if name.ends_with(".partial") {
                    return None;
                }
                let (files, bytes) = tree_size(&e.path());
                Some(Artifact {
                    name,
                    files,
                    bytes,
                    path: e.path(),
                })
            })
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    pub fn save_summary(&self, record: &RunRecord) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        write_atomic(
            &self.summary_path(&record.id),
            &serde_json::to_vec_pretty(record)?,
        )
    }

    pub fn append_logs(&self, id: &str, lines: &[LogLine]) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.logs_path(id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("writing {}", path.display()))?;
        let mut buf = Vec::new();
        for line in lines {
            serde_json::to_writer(&mut buf, line)?;
            buf.push(b'\n');
        }
        file.write_all(&buf)?;
        Ok(())
    }

    /// One run by id, with a run whose process died reported as interrupted.
    pub fn get(&self, id: &str) -> Option<RunRecord> {
        let bytes = fs::read(self.summary_path(id)).ok()?;
        serde_json::from_slice::<RunRecord>(&bytes)
            .ok()
            .map(settle_stale)
    }

    fn all(&self) -> Vec<RunRecord> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut runs: Vec<RunRecord> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|p| serde_json::from_slice::<RunRecord>(&fs::read(p).ok()?).ok())
            .map(settle_stale)
            .collect();
        runs.sort_by(|a, b| b.started.cmp(&a.started).then_with(|| b.id.cmp(&a.id)));
        runs
    }

    /// Runs for `project`, newest first. Unreadable files are skipped.
    pub fn list(&self, project: &Path) -> Vec<RunRecord> {
        self.all()
            .into_iter()
            .filter(|r| r.project == project)
            .collect()
    }

    /// Finds a run by `last` (newest of `project`), its full id, or a unique id prefix.
    pub fn find(&self, project: &Path, query: &str) -> Result<RunRecord> {
        if query == "last" || query == "latest" {
            return self
                .list(project)
                .into_iter()
                .next()
                .context("this project has no runs yet");
        }
        if let Some(run) = self.get(query) {
            return Ok(run);
        }
        let mut matches: Vec<RunRecord> = self
            .all()
            .into_iter()
            .filter(|r| r.id.starts_with(query))
            .collect();
        match matches.len() {
            0 => bail!("no run with id `{query}`"),
            1 => Ok(matches.remove(0)),
            n => bail!("`{query}` matches {n} runs; use more of the id"),
        }
    }

    pub fn logs(&self, id: &str) -> Vec<LogLine> {
        self.logs_from(id, 0).0
    }

    /// Log lines starting at byte `offset`, and the offset to continue from.
    pub fn logs_from(&self, id: &str, offset: u64) -> (Vec<LogLine>, u64) {
        let Ok(mut file) = fs::File::open(self.logs_path(id)) else {
            return (Vec::new(), offset);
        };
        if file.seek(SeekFrom::Start(offset)).is_err() {
            return (Vec::new(), offset);
        }
        let mut reader = BufReader::new(file);
        let mut lines = Vec::new();
        let mut pos = offset;
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) | Err(_) => break,
                // A line still being written has no newline yet; pick it up next time.
                Ok(_) if !buf.ends_with('\n') => break,
                Ok(n) => {
                    pos += n as u64;
                    if let Ok(line) = serde_json::from_str(buf.trim_end()) {
                        lines.push(line);
                    }
                }
            }
        }
        (lines, pos)
    }

    /// Asks the process running `id` to stop; it checks for this while it runs.
    pub fn request_cancel(&self, id: &str) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        fs::write(self.cancel_path(id), b"")?;
        Ok(())
    }

    pub fn cancel_requested(&self, id: &str) -> bool {
        self.cancel_path(id).exists()
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        for path in [
            self.summary_path(id),
            self.logs_path(id),
            self.cancel_path(id),
        ] {
            match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("deleting {}", path.display()))
                }
                _ => {}
            }
        }
        match fs::remove_dir_all(self.dir.join(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("deleting the artifacts of {id}"))
            }
            _ => Ok(()),
        }
    }

    /// Deletes every finished run of `project`; returns how many were removed.
    pub fn clear(&self, project: &Path) -> Result<usize> {
        let runs: Vec<RunRecord> = self
            .list(project)
            .into_iter()
            .filter(|r| r.outcome.is_done())
            .collect();
        for run in &runs {
            self.delete(&run.id)?;
        }
        Ok(runs.len())
    }
}

fn settle_stale(mut record: RunRecord) -> RunRecord {
    if record.outcome == Outcome::Running && now().saturating_sub(record.updated) > STALE_SECS {
        record.outcome = Outcome::Interrupted;
    }
    record
}

/// The number of files under `dir` and their total size.
fn tree_size(dir: &Path) -> (usize, u64) {
    let mut total = (0, 0);
    for entry in fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            let (files, bytes) = tree_size(&entry.path());
            total = (total.0 + files, total.1 + bytes);
        } else {
            total = (total.0 + 1, total.1 + meta.len());
        }
    }
    total
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// Where a run came from, for its record.
pub struct RunMeta {
    pub project: PathBuf,
    pub workflow: PathBuf,
    pub job: Option<String>,
    pub event: String,
    pub engine: String,
}

/// Keeps a run's record in the store up to date while it runs. Feed it every runner event,
/// call [`Recorder::tick`] regularly, and [`Recorder::finish`] once.
pub struct Recorder {
    store: Store,
    record: RunRecord,
    started: Instant,
    job_started: Vec<Option<Instant>>,
    step_started: Vec<Vec<Option<Instant>>>,
    pending: Vec<LogLine>,
    flushed: Instant,
    finished: bool,
    error: Option<String>,
}

impl Recorder {
    pub fn begin(store: Store, id: String, plan: &RunPlan, meta: RunMeta) -> Self {
        let record = RunRecord {
            id,
            project: meta.project,
            title: plan.title.clone(),
            workflow: meta.workflow,
            job: meta.job,
            engine: meta.engine,
            event: meta.event,
            started: now(),
            updated: now(),
            millis: 0,
            outcome: Outcome::Running,
            jobs: plan
                .jobs
                .iter()
                .map(|j| JobRecord {
                    name: j.label.clone(),
                    status: Status::Pending,
                    millis: None,
                    steps: j
                        .step_names
                        .iter()
                        .map(|s| StepRecord {
                            name: s.clone(),
                            status: Status::Pending,
                            millis: None,
                        })
                        .collect(),
                })
                .collect(),
        };
        let mut rec = Self {
            job_started: vec![None; record.jobs.len()],
            step_started: record
                .jobs
                .iter()
                .map(|j| vec![None; j.steps.len()])
                .collect(),
            store,
            record,
            started: Instant::now(),
            pending: Vec::new(),
            flushed: Instant::now(),
            finished: false,
            error: None,
        };
        rec.flush();
        rec
    }

    /// The first error writing to the store, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn apply(&mut self, event: &RunEvent) {
        match event {
            RunEvent::Job { job, status } => {
                let j = &mut self.record.jobs[*job];
                if *status == Status::Running {
                    self.job_started[*job] = Some(Instant::now());
                } else if status.is_done() {
                    j.millis = self.job_started[*job].map(|s| s.elapsed().as_millis() as u64);
                }
                j.status = *status;
            }
            RunEvent::Step { job, step, status } => {
                let s = &mut self.record.jobs[*job].steps[*step];
                if *status == Status::Running {
                    self.step_started[*job][*step] = Some(Instant::now());
                } else if status.is_done() {
                    s.millis =
                        self.step_started[*job][*step].map(|t| t.elapsed().as_millis() as u64);
                }
                s.status = *status;
            }
            RunEvent::Log {
                job,
                step,
                kind,
                text,
            } => self.pending.push(LogLine {
                job: *job,
                step: *step,
                kind: *kind,
                text: text.clone(),
            }),
            RunEvent::Finished { .. } => {}
        }
    }

    /// Writes pending changes when due. Returns true once someone asked to cancel the run.
    pub fn tick(&mut self) -> bool {
        if self.flushed.elapsed() >= FLUSH_EVERY {
            self.flush();
        }
        self.store.cancel_requested(&self.record.id)
    }

    /// Saves the final record (once). Anything still pending or running counts as cancelled.
    pub fn finish(&mut self, outcome: Outcome) {
        if self.finished {
            return;
        }
        self.finished = true;
        let settle = |s: Status| if s.is_done() { s } else { Status::Cancelled };
        for (j, job) in self.record.jobs.iter_mut().enumerate() {
            if !job.status.is_done() {
                job.millis = self.job_started[j].map(|s| s.elapsed().as_millis() as u64);
            }
            job.status = settle(job.status);
            for (s, step) in job.steps.iter_mut().enumerate() {
                if !step.status.is_done() {
                    step.millis = self.step_started[j][s].map(|t| t.elapsed().as_millis() as u64);
                }
                step.status = settle(step.status);
            }
        }
        self.record.outcome = outcome;
        self.flush();
        let _ = fs::remove_file(self.store.cancel_path(&self.record.id));
        // Every job mounts the artifacts folder; drop it again when nothing was uploaded.
        let artifacts = self.store.artifacts_dir(&self.record.id);
        if fs::remove_dir(&artifacts).is_ok() {
            let _ = fs::remove_dir(artifacts.parent().unwrap_or(&artifacts));
        }
    }

    fn flush(&mut self) {
        self.flushed = Instant::now();
        self.record.updated = now();
        self.record.millis = self.started.elapsed().as_millis() as u64;
        let logs = std::mem::take(&mut self.pending);
        let result = self
            .store
            .append_logs(&self.record.id, &logs)
            .and_then(|_| self.store.save_summary(&self.record));
        if let Err(e) = result {
            if self.error.is_none() {
                self.error = Some(format!("Could not save this run to the history: {e:#}"));
            }
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.finish(Outcome::Cancelled);
    }
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
            engine: "Docker".into(),
            event: "push".into(),
            started,
            updated: now(),
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
        store.save_summary(&older).unwrap();
        store.append_logs(&older.id, &[line]).unwrap();
        store.save_summary(&newer).unwrap();
        store
            .save_summary(&record(Path::new("/projects/other"), 300))
            .unwrap();

        let runs = store.list(project);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].id, newer.id, "newest first");
        assert_eq!(store.find(project, "last").unwrap().id, newer.id);
        assert_eq!(store.logs(&older.id)[0].text, "hello");

        let kept = store.artifacts_dir(&older.id).join("report");
        fs::create_dir_all(kept.join("sub")).unwrap();
        fs::write(kept.join("a.txt"), "hi").unwrap();
        fs::write(kept.join("sub/b.txt"), "there").unwrap();
        let artifacts = store.artifacts(&older.id);
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            (
                artifacts[0].name.as_str(),
                artifacts[0].files,
                artifacts[0].bytes
            ),
            ("report", 2, 7)
        );
        assert_eq!(
            store.list(project).len(),
            2,
            "artifact folders are not runs"
        );

        store.delete(&older.id).unwrap();
        assert!(store.artifacts(&older.id).is_empty());
        assert_eq!(store.list(project).len(), 1);
        assert!(store.logs(&older.id).is_empty());
        assert_eq!(store.clear(project).unwrap(), 1);
        assert!(store.list(project).is_empty());
        assert_eq!(store.list(Path::new("/projects/other")).len(), 1);
    }

    #[test]
    fn logs_are_read_incrementally() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let line = |text: &str| LogLine {
            job: 0,
            step: 0,
            kind: LogKind::Output,
            text: text.into(),
        };
        store.append_logs("r", &[line("one")]).unwrap();
        let (first, offset) = store.logs_from("r", 0);
        assert_eq!(first.len(), 1);
        store
            .append_logs("r", &[line("two"), line("three")])
            .unwrap();
        let (rest, _) = store.logs_from("r", offset);
        assert_eq!(
            rest.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["two", "three"]
        );
    }

    #[test]
    fn stale_running_runs_read_as_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let mut run = record(Path::new("/p"), 100);
        run.outcome = Outcome::Running;
        run.updated = now() - 120;
        store.save_summary(&run).unwrap();
        assert_eq!(store.get(&run.id).unwrap().outcome, Outcome::Interrupted);
        assert!(store.request_cancel(&run.id).is_ok());
        assert!(store.cancel_requested(&run.id));
    }
}
