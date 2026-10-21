//! Turns workflows into a concrete list of jobs: matrix expanded, dependencies resolved.

use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::expr::{self, Ctx, JobState};
use super::model::{Job, Step, Workflow};

pub struct PlannedJob {
    pub workflow: Arc<Workflow>,
    pub workflow_name: String,
    pub key: String,
    pub label: String,
    pub job: Job,
    pub matrix: Value,
    pub runs_on: Vec<String>,
    /// Indices of jobs in the plan that must finish first.
    pub needs: Vec<usize>,
    /// UI step names: "Set up job", each step, "Complete job".
    pub step_names: Vec<String>,
    pub fail_fast: bool,
    pub matrix_index: usize,
    pub matrix_total: usize,
}

pub struct RunPlan {
    pub title: String,
    pub jobs: Vec<PlannedJob>,
}

impl RunPlan {
    /// Builds a plan for the given workflows. With `job_filter`, only that job (all its matrix
    /// variants) is planned and its `needs` are treated as already satisfied.
    pub fn build(sources: &[(Arc<Workflow>, String)], job_filter: Option<&str>) -> Result<RunPlan> {
        let multi = sources.len() > 1;
        let mut jobs: Vec<PlannedJob> = Vec::new();
        for (wf, wf_name) in sources {
            let base = jobs.len();
            for (key, job) in &wf.jobs {
                if let Some(filter) = job_filter {
                    let by_name = job
                        .name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(filter));
                    if key != filter && !by_name {
                        continue;
                    }
                }
                let combos = expand_matrix(job).map_err(|e| anyhow::anyhow!("job `{key}`: {e}"))?;
                let total = combos.len();
                for (i, combo) in combos.into_iter().enumerate() {
                    let matrix = Value::Object(combo);
                    let data = json!({ "matrix": matrix });
                    let ctx = Ctx::new(&data, JobState::default());
                    let mut label = job_label(key, job, &matrix, &ctx);
                    if multi {
                        label = format!("{wf_name} / {label}");
                    }
                    let mut step_names = vec!["Set up job".to_string()];
                    step_names.extend(job.steps.iter().map(|s| step_title(s, &ctx)));
                    step_names.push("Complete job".to_string());
                    jobs.push(PlannedJob {
                        workflow: wf.clone(),
                        workflow_name: wf_name.clone(),
                        key: key.clone(),
                        label,
                        job: job.clone(),
                        runs_on: job
                            .runs_on()
                            .iter()
                            .map(|l| expr::interpolate(l, &ctx).unwrap_or_else(|_| l.clone()))
                            .collect(),
                        matrix,
                        needs: Vec::new(),
                        step_names,
                        fail_fast: fail_fast(job),
                        matrix_index: i,
                        matrix_total: total,
                    });
                }
            }
            for idx in base..jobs.len() {
                let mut resolved = Vec::new();
                for need in jobs[idx].job.needs() {
                    let targets: Vec<usize> = (base..jobs.len())
                        .filter(|&j| jobs[j].key == need)
                        .collect();
                    if targets.is_empty() && job_filter.is_none() {
                        bail!("job `{}` needs unknown job `{need}`", jobs[idx].key);
                    }
                    resolved.extend(targets);
                }
                jobs[idx].needs = resolved;
            }
        }
        if jobs.is_empty() {
            match job_filter {
                Some(j) => bail!("no job named `{j}` in the selected workflow(s)"),
                None => bail!("the selected workflow(s) have no jobs"),
            }
        }
        check_acyclic(&jobs)?;
        let title = match (sources, job_filter) {
            ([(_, name)], Some(job)) => format!("{name} › {job}"),
            ([(_, name)], None) => name.clone(),
            (all, _) => format!("{} workflows", all.len()),
        };
        Ok(RunPlan { title, jobs })
    }
}

fn check_acyclic(jobs: &[PlannedJob]) -> Result<()> {
    let mut indegree: Vec<usize> = jobs.iter().map(|j| j.needs.len()).collect();
    let mut ready: Vec<usize> = (0..jobs.len()).filter(|&i| indegree[i] == 0).collect();
    let mut seen = 0;
    while let Some(i) = ready.pop() {
        seen += 1;
        for (j, job) in jobs.iter().enumerate() {
            for _ in job.needs.iter().filter(|&&n| n == i) {
                indegree[j] -= 1;
                if indegree[j] == 0 {
                    ready.push(j);
                }
            }
        }
    }
    if seen != jobs.len() {
        bail!("job dependencies (`needs`) form a cycle");
    }
    Ok(())
}

fn fail_fast(job: &Job) -> bool {
    match job.strategy.as_ref().and_then(|s| s.fail_fast.as_ref()) {
        Some(serde_yaml::Value::Bool(b)) => *b,
        _ => true,
    }
}
