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

fn job_label(key: &str, job: &Job, matrix: &Value, ctx: &Ctx) -> String {
    let base = match &job.name {
        Some(n) => expr::interpolate(n, ctx).unwrap_or_else(|_| n.clone()),
        None => key.to_string(),
    };
    let values: Vec<String> = matrix
        .as_object()
        .map(|m| {
            m.values()
                .map(expr::to_str)
                .map(|s| s.replace('\n', " "))
                .collect()
        })
        .unwrap_or_default();
    let name_uses_matrix = job.name.as_deref().is_some_and(|n| n.contains("matrix."));
    if values.is_empty() || name_uses_matrix {
        base
    } else {
        format!("{base} ({})", values.join(", "))
    }
}

/// Human-readable step title, mirroring GitHub's defaults.
pub fn step_title(step: &Step, ctx: &Ctx) -> String {
    if let Some(name) = &step.name {
        return expr::interpolate(name, ctx).unwrap_or_else(|_| name.clone());
    }
    if let Some(uses) = &step.uses {
        return format!("Run {uses}");
    }
    if let Some(run) = &step.run {
        let first = run
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("");
        let mut title: String = first.chars().take(60).collect();
        if first.chars().count() > 60 {
            title.push('…');
        }
        return format!("Run {title}");
    }
    "Step".to_string()
}

/// Expands `strategy.matrix` into concrete combinations (one empty combination if there's none).
pub fn expand_matrix(job: &Job) -> Result<Vec<Map<String, Value>>, String> {
    let Some(matrix) = job.strategy.as_ref().and_then(|s| s.matrix.as_ref()) else {
        return Ok(vec![Map::new()]);
    };
    let matrix: Value = serde_json::to_value(matrix).map_err(|e| e.to_string())?;
    let Some(obj) = matrix.as_object() else {
        return Err("dynamic matrices (`${{ fromJSON(...) }}`) are not supported yet".into());
    };
    let base_keys: Vec<&String> = obj
        .keys()
        .filter(|k| *k != "include" && *k != "exclude")
        .collect();
    let mut combos: Vec<Map<String, Value>> = Vec::new();
    if !base_keys.is_empty() {
        combos.push(Map::new());
        for key in &base_keys {
            let values = obj[key.as_str()]
                .as_array()
                .ok_or_else(|| format!("matrix key `{key}` must be a list"))?;
            combos = combos
                .into_iter()
                .flat_map(|c| {
                    values.iter().map(move |v| {
                        let mut c = c.clone();
                        c.insert((*key).clone(), v.clone());
                        c
                    })
                })
                .collect();
        }
    }
    if let Some(excludes) = obj.get("exclude").and_then(|e| e.as_array()) {
        combos.retain(|c| {
            !excludes
                .iter()
                .filter_map(|e| e.as_object())
                .any(|ex| ex.iter().all(|(k, v)| c.get(k) == Some(v)))
        });
    }
    if let Some(includes) = obj.get("include").and_then(|i| i.as_array()) {
        let original = combos.len();
        for inc in includes.iter().filter_map(|i| i.as_object()) {
            let mut matched = false;
            for c in combos.iter_mut().take(original) {
                let compatible = inc
                    .iter()
                    .filter(|(k, _)| base_keys.contains(k))
                    .all(|(k, v)| c.get(k) == Some(v));
                if compatible {
                    for (k, v) in inc.iter().filter(|(k, _)| !base_keys.contains(k)) {
                        c.insert(k.clone(), v.clone());
                    }
                    matched = true;
                }
            }
            if !matched {
                combos.push(inc.clone());
            }
        }
    }
    if combos.is_empty() && base_keys.is_empty() {
        combos.push(Map::new());
    }
    Ok(combos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(yaml: &str) -> Job {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn cartesian_with_include_exclude() {
        let j = job(r#"
strategy:
  matrix:
    os: [linux, mac]
    node: [18, 20]
    exclude:
      - os: mac
        node: 18
    include:
      - os: linux
        experimental: true
      - os: windows
        node: 20
"#);
        let combos = expand_matrix(&j).unwrap();
        assert_eq!(combos.len(), 4);
        assert_eq!(
            combos
                .iter()
                .filter(|c| c.contains_key("experimental"))
                .count(),
            2
        );
        assert_eq!(combos[3]["os"], "windows");
    }

    #[test]
    fn no_matrix_is_single_job() {
        assert_eq!(expand_matrix(&job("steps: []")).unwrap().len(), 1);
    }

    #[test]
    fn plan_resolves_needs_and_detects_cycles() {
        let wf: Workflow = serde_yaml::from_str(
            r#"
on: push
jobs:
  build:
    runs-on: ubuntu-latest
    strategy: { matrix: { v: [1, 2] } }
    steps: [{ run: echo hi }]
  test:
    needs: build
    runs-on: ubuntu-latest
    steps: [{ name: "Test ${{ matrix.v }}", run: echo }]
"#,
        )
        .unwrap();
        let plan = RunPlan::build(&[(Arc::new(wf), "CI".into())], None).unwrap();
        assert_eq!(plan.jobs.len(), 3);
        assert_eq!(plan.jobs[2].needs, vec![0, 1]);
        assert_eq!(plan.jobs[0].label, "build (1)");
        assert_eq!(
            plan.jobs[0].step_names,
            vec!["Set up job", "Run echo hi", "Complete job"]
        );

        let cyclic: Workflow = serde_yaml::from_str(
            "on: push\njobs:\n  a: { needs: b, steps: [] }\n  b: { needs: a, steps: [] }\n",
        )
        .unwrap();
        assert!(RunPlan::build(&[(Arc::new(cyclic), "x".into())], None).is_err());
    }
}
