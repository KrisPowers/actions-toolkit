use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Semaphore;

use crate::app::AppState;
use crate::db::queries::runs as run_queries;
use crate::db::queries::settings as settings_queries;
use crate::runner::executor::{self, CheckoutContext};
use crate::workflow::expr::{evaluate, ExprContext};
use crate::workflow::model::Workflow;

/// Drive an entire workflow run to completion: repeatedly finds jobs whose `needs` are all
/// satisfied and dispatches them (bounded by `max_concurrent_jobs`), skips jobs downstream of
/// a failure (unless they opt in via `if: always()`), and marks the run terminal once every
/// job has reached a terminal state.
///
/// `repo_owner`/`repo_name`/`commit_sha` exist only to report the run's outcome back to GitHub
/// as a commit status (see `github_status::report_success`/`report_failure`) once it finishes;
/// `commit_sha` is `None` for runs with no specific commit (e.g. manual dispatch against no
/// particular ref), in which case that reporting is skipped entirely.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    state: Arc<AppState>,
    workflow_run_id: String,
    workflow: Workflow,
    checkout: Option<CheckoutContext>,
    trigger_event: String,
    repo_owner: String,
    repo_name: String,
    commit_sha: Option<String>,
) {
    let result = run_inner(&state, &workflow_run_id, &workflow, checkout, &trigger_event).await;
    let succeeded = match &result {
        Ok(succeeded) => *succeeded,
        Err(e) => {
            tracing::error!(error = %e, workflow_run_id, "workflow run failed");
            let _ = run_queries::set_run_status(&state.db, &workflow_run_id, "failed", true).await;
            false
        }
    };

    if let Some(sha) = commit_sha {
        let target_url = crate::runner::github_status::run_target_url(&state, &workflow_run_id).await;
        let report = if succeeded {
            crate::runner::github_status::report_success(&state, &repo_owner, &repo_name, &sha, target_url).await
        } else {
            crate::runner::github_status::report_failure(&state, &repo_owner, &repo_name, &sha, target_url).await
        };
        if let Err(e) = report {
            tracing::warn!(error = format!("{e:#}"), workflow_run_id, sha, "failed to post the final GitHub commit status");
        }
    }
}

/// Returns whether every job in the run succeeded.
async fn run_inner(
    state: &Arc<AppState>,
    workflow_run_id: &str,
    workflow: &Workflow,
    checkout: Option<CheckoutContext>,
    trigger_event: &str,
) -> Result<bool> {
    if !state.bucket_capability_ok {
        run_queries::set_run_status(&state.db, workflow_run_id, "failed", true).await?;
        anyhow::bail!("Bucket sandbox is not available on this host; cannot run jobs without a container:");
    }
    let docker = state.docker.clone();

    run_queries::set_run_status(&state.db, workflow_run_id, "running", false).await?;

    let job_runs = run_queries::list_job_runs(&state.db, workflow_run_id).await?;
    let mut job_run_ids: HashMap<String, String> = HashMap::new(); // job_key -> job_run_id
    for jr in &job_runs {
        job_run_ids.insert(jr.job_key.clone(), jr.id.clone());
    }

    let max_concurrent_jobs = settings_queries::get(&state.db).await?.max_concurrent_jobs.max(1) as usize;
    let semaphore = Arc::new(Semaphore::new(max_concurrent_jobs));
    let mut results: HashMap<String, bool> = HashMap::new();
    let mut in_flight: HashSet<String> = HashSet::new();
    let mut handles = Vec::new();

    loop {
        let pending: Vec<String> = job_runs
            .iter()
            .filter(|jr| !results.contains_key(&jr.job_key) && !in_flight.contains(&jr.job_key))
            .map(|jr| jr.job_key.clone())
            .collect();

        if pending.is_empty() && handles.is_empty() {
            break;
        }

        let mut dispatched_any = false;

        for job_key in pending {
            let Some(job) = workflow.jobs.get(&job_key) else { continue };
            let deps_done = job.needs.iter().all(|n| results.contains_key(n));
            if !deps_done {
                continue;
            }

            let any_failed_dep = job.needs.iter().any(|n| results.get(n) == Some(&false));
            let mut ctx = ExprContext::new();
            ctx.any_failed_dependency = any_failed_dep;
            ctx.set("github.event_name", trigger_event);
            for need in &job.needs {
                ctx.job_results.insert(
                    need.clone(),
                    if results.get(need).copied().unwrap_or(false) { "success" } else { "failure" }.to_string(),
                );
            }
            let should_run = job
                .if_condition
                .as_deref()
                .map(|expr| evaluate(expr, &ctx))
                .unwrap_or(!any_failed_dep);

            let job_run_id = job_run_ids.get(&job_key).cloned().unwrap();

            if !should_run {
                run_queries::set_job_status(&state.db, &job_run_id, "skipped", None, true).await?;
                results.insert(job_key.clone(), false);
                dispatched_any = true;
                continue;
            }

            dispatched_any = true;
            let permit = semaphore.clone().acquire_owned().await?;
            let state = state.clone();
            let docker = docker.clone();
            let workflow_run_id = workflow_run_id.to_string();
            let job = job.clone();
            let checkout = checkout.as_ref().map(|c| CheckoutContext {
                owner: c.owner.clone(),
                repo: c.repo.clone(),
                repo_id: c.repo_id.clone(),
                pat: c.pat.clone(),
                git_ref: c.git_ref.clone(),
            });
            let job_key_owned = job_key.clone();
            in_flight.insert(job_key.clone());

            let handle = tokio::spawn(async move {
                let _permit = permit;
                let outcome = executor::run_job(&state, &docker, &workflow_run_id, &job_run_id, &job, checkout)
                    .await
                    .unwrap_or(false);
                (job_key_owned, outcome)
            });
            handles.push(handle);
        }

        if !dispatched_any && !handles.is_empty() {
            // Wait for at least one in-flight job to finish before re-evaluating readiness.
            let (result, _index, remaining) = futures::future::select_all(handles).await;
            handles = remaining;
            if let Ok((job_key, outcome)) = result {
                results.insert(job_key, outcome);
            }
        } else if !dispatched_any && handles.is_empty() {
            // Nothing left that can ever become ready (e.g. bad `needs` graph); bail out to
            // avoid an infinite loop.
            break;
        }
    }

    // Drain any still-running jobs.
    for handle in handles {
        if let Ok((job_key, outcome)) = handle.await {
            results.insert(job_key, outcome);
        }
    }

    let all_succeeded = results.values().all(|ok| *ok);
    run_queries::set_run_status(
        &state.db,
        workflow_run_id,
        if all_succeeded { "succeeded" } else { "failed" },
        true,
    )
    .await?;

    // Artifacts have already been copied out to data/artifacts/; each job's own checked-out
    // workspace (keyed by job_run_id, not workflow_run_id, see executor::run_job) is no longer
    // needed once every job has reached a terminal state.
    for job_run_id in job_run_ids.values() {
        crate::runner::workspace::cleanup(&state.config.workspaces_dir(), job_run_id);
    }

    Ok(all_succeeded)
}
