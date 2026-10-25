//! Structured views of workflows and runs, shared by `--json` output and the AI tools, so a
//! person reading `atk status` and an agent calling `get_run` see the same facts.

use std::path::Path;

use anyhow::{bail, Result};
use serde_json::{json, Value};

use crate::history::{LogLine, RunRecord, Store};
use crate::runner::{LogKind, Status};
use crate::workflow::{self, plan::RunPlan, WorkflowFile};

/// Lines of output kept from a failed step in a run report.
const FAILURE_TAIL: usize = 25;

/// `path` relative to `root` when inside it, with forward slashes.
pub fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every workflow of the project with its triggers and jobs.
pub fn workflows(root: &Path) -> Value {
    let files = workflow::discover(root);
    Value::Array(files.iter().map(|f| workflow_entry(root, f)).collect())
}

fn workflow_entry(root: &Path, f: &WorkflowFile) -> Value {
    let path = rel(root, &f.path);
    let Some(wf) = f.workflow() else {
        return json!({
            "name": f.file_name,
            "path": path,
            "source": f.source.label(),
            "error": f.parsed.as_ref().err(),
        });
    };
    let jobs: Vec<Value> = wf
        .jobs
        .iter()
        .map(|(key, job)| {
            json!({
                "id": key,
                "name": job.name,
                "runs_on": job.runs_on(),
                "needs": job.needs(),
                "steps": job.steps.len(),
                "matrix": workflow::plan::expand_matrix(job).map(|c| c.len()).unwrap_or(1),
            })
        })
        .collect();
    json!({
        "name": f.display_name(),
        "path": path,
        "source": f.source.label(),
        "on": f.events(),
        "jobs": jobs,
    })
}

/// Parses and plans every workflow (or just `only`); returns the results and whether all
/// of them are valid.
pub fn validate(root: &Path, only: Option<&Path>) -> (Value, bool) {
    let files: Vec<WorkflowFile> = workflow::discover(root)
        .into_iter()
        .filter(|f| only.is_none_or(|p| same_file(p, &f.path)))
        .collect();
    let mut all_ok = true;
    let results = files
        .iter()
        .map(|f| {
            let checked: Result<RunPlan, String> = f.parsed.clone().and_then(|wf| {
                RunPlan::build(&[(wf, f.display_name())], None).map_err(|e| format!("{e:#}"))
            });
            all_ok &= checked.is_ok();
            match checked {
                Ok(plan) => json!({
                    "path": rel(root, &f.path),
                    "name": f.display_name(),
                    "valid": true,
                    "jobs": plan.jobs.len(),
                }),
                Err(e) => json!({ "path": rel(root, &f.path), "valid": false, "error": e }),
            }
        })
        .collect();
    (Value::Array(results), all_ok)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// A run's summary: workflow, outcome, every job and step with its status and duration.
pub fn run(record: &RunRecord) -> Value {
    let mut v = serde_json::to_value(record).unwrap_or(Value::Null);
    v["outcome"] = json!(record.outcome.as_str());
    v
}

/// A run's summary plus, for every failed step, its error lines and the end of its output:
/// enough to review a failure without reading the whole log.
pub fn review(store: &Store, record: &RunRecord) -> Value {
    let logs = store.logs(&record.id);
    let mut failures = Vec::new();
    for (j, job) in record.jobs.iter().enumerate() {
        for (s, step) in job.steps.iter().enumerate() {
            if step.status != Status::Failure {
                continue;
            }
            let lines: Vec<&LogLine> = logs.iter().filter(|l| l.job == j && l.step == s).collect();
            let errors: Vec<&str> = lines
                .iter()
                .filter(|l| l.kind == LogKind::Error)
                .map(|l| l.text.as_str())
                .collect();
            let output: Vec<&str> = lines
                .iter()
                .filter(|l| !matches!(l.kind, LogKind::Header | LogKind::Group))
                .map(|l| l.text.as_str())
                .collect();
            let tail = &output[output.len().saturating_sub(FAILURE_TAIL)..];
            failures.push(json!({
                "job": job.name,
                "step": step.name,
                "errors": errors,
                "output_tail": tail,
            }));
        }
    }
    let mut v = run(record);
    v["failures"] = Value::Array(failures);
    v
}

/// One log line with its job and step named.
pub fn log_line(record: &RunRecord, line: &LogLine) -> Value {
    let job = record.jobs.get(line.job);
    json!({
        "job": job.map(|j| j.name.as_str()),
        "step": job.and_then(|j| j.steps.get(line.step)).map(|s| s.name.as_str()),
        "kind": line.kind,
        "text": line.text,
    })
}

/// Indices of the jobs a `--job` filter picks: a job's number, its name, or the start of
/// its name (all matrix variants of `test` for `test`).
pub fn pick_jobs(record: &RunRecord, query: &str) -> Result<Vec<usize>> {
    if let Ok(i) = query.parse::<usize>() {
        if i < record.jobs.len() {
            return Ok(vec![i]);
        }
    }
    let q = query.to_lowercase();
    let names: Vec<String> = record.jobs.iter().map(|j| j.name.to_lowercase()).collect();
    let exact: Vec<usize> = (0..names.len()).filter(|&i| names[i] == q).collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    let prefix: Vec<usize> = (0..names.len())
        .filter(|&i| names[i].starts_with(&q))
        .collect();
    if prefix.is_empty() {
        let all: Vec<&str> = record.jobs.iter().map(|j| j.name.as_str()).collect();
        bail!("no job `{query}` in this run (jobs: {})", all.join(", "));
    }
    Ok(prefix)
}

/// The log lines of a run, narrowed to some jobs and one step (by number or name).
pub fn filter_logs(
    record: &RunRecord,
    logs: Vec<LogLine>,
    job: Option<&str>,
    step: Option<&str>,
) -> Result<Vec<LogLine>> {
    let jobs = match job {
        Some(q) => Some(pick_jobs(record, q)?),
        None => None,
    };
    let step = step.map(|s| s.to_lowercase());
    Ok(logs
        .into_iter()
        .filter(|l| jobs.as_ref().is_none_or(|js| js.contains(&l.job)))
        .filter(|l| {
            step.as_ref().is_none_or(|q| {
                let name = record
                    .jobs
                    .get(l.job)
                    .and_then(|j| j.steps.get(l.step))
                    .map(|s| s.name.to_lowercase());
                q.parse::<usize>().ok() == Some(l.step) || name.as_deref() == Some(q.as_str())
            })
        })
        .collect())
}
