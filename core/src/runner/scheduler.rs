use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use bollard::Docker;
use tokio::sync::Semaphore;

use crate::app::AppState;
use crate::db::queries::runs as run_queries;
use crate::runner::executor::{self, CheckoutContext};
use crate::runner::run_client::RunClient;
use crate::workflow::expr::{evaluate, ExprContext};
use crate::workflow::model::Workflow;

/// Control-plane side of running a workflow: spawns the shell subprocess that actually drives the
/// job DAG (see `run_inner`, which now runs *inside* that subprocess, not here), waits for it to
/// exit, then reports the run's outcome back to GitHub — both as a commit status
/// (`github_status::report_success`/`report_failure`) and, when `check_run_id` is `Some` (the
/// check started fine back in `dispatch::spawn_run`), by completing that GitHub check run too.
/// The GitHub client (the instance-wide App token) is a control-plane-only credential, so this
/// step deliberately stays here rather than moving into the shell along with everything else.
/// `commit_sha` is `None` for runs with no specific commit (e.g. manual dispatch against no
/// particular ref), in which case all of that reporting is skipped entirely.
#[allow(clippy::too_many_arguments)]
pub async fn supervise_shell(
    state: Arc<AppState>,
    mut child: tokio::process::Child,
    workflow_run_id: String,
    repo_owner: String,
    repo_name: String,
    commit_sha: Option<String>,
    check_run_id: Option<u64>,
) {
    match child.wait().await {
        Ok(status) if !status.success() => {
            tracing::warn!(workflow_run_id, code = ?status.code(), "shell process exited non-zero");
        }
        Err(e) => {
            tracing::error!(error = %e, workflow_run_id, "failed waiting for the shell process");
        }
        _ => {}
    }

    report_run_outcome(&state, &workflow_run_id, &repo_owner, &repo_name, commit_sha, check_run_id).await;
}

/// Same GitHub reporting `supervise_shell` does, but for a shell scheduled onto a remote agent:
/// there's no local child process to await, so this polls the run's own status instead, stopping
/// once it reaches a terminal state (a shell only sets one via `set_run_status` at the very end of
/// `run_inner`, so seeing one here already means the run is fully done, not just close to it).
const REMOTE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
const REMOTE_POLL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

#[allow(clippy::too_many_arguments)]
pub async fn supervise_remote_shell(
    state: Arc<AppState>,
    workflow_run_id: String,
    repo_owner: String,
    repo_name: String,
    commit_sha: Option<String>,
    check_run_id: Option<u64>,
) {
    let deadline = tokio::time::Instant::now() + REMOTE_POLL_TIMEOUT;
    loop {
        match run_queries::find_run(&state.db, &workflow_run_id).await {
            Ok(Some(run)) if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") => break,
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, workflow_run_id, "failed polling a remote shell's run status"),
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!(workflow_run_id, "gave up polling a remote shell's run status; Janga's sweep on the agent side is the backstop for a shell that never reports back");
            return;
        }
        tokio::time::sleep(REMOTE_POLL_INTERVAL).await;
    }

    report_run_outcome(&state, &workflow_run_id, &repo_owner, &repo_name, commit_sha, check_run_id).await;
}

async fn report_run_outcome(
    state: &AppState,
    workflow_run_id: &str,
    repo_owner: &str,
    repo_name: &str,
    commit_sha: Option<String>,
    check_run_id: Option<u64>,
) {
    let succeeded = run_queries::find_run(&state.db, workflow_run_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.status == "succeeded")
        .unwrap_or(false);

    if let Some(sha) = commit_sha {
        let target_url = crate::runner::github_status::run_target_url(state, workflow_run_id).await;
        let report = if succeeded {
            crate::runner::github_status::report_success(state, repo_owner, repo_name, &sha, target_url).await
        } else {
            crate::runner::github_status::report_failure(state, repo_owner, repo_name, &sha, target_url).await
        };
        if let Err(e) = report {
            tracing::warn!(error = format!("{e:#}"), workflow_run_id, sha, "failed to post the final GitHub commit status");
        }

        if let Some(check_run_id) = check_run_id {
            if let Err(e) = crate::runner::github_status::complete_check(state, repo_owner, repo_name, check_run_id, succeeded).await {
                tracing::warn!(error = format!("{e:#}"), workflow_run_id, check_run_id, "failed to complete the GitHub check run");
            }
        }
    }
}

/// Drives an entire workflow run to completion from inside its own shell subprocess: repeatedly
/// finds jobs whose `needs` are all satisfied and dispatches them (bounded by
/// `max_concurrent_jobs`), skips jobs downstream of a failure (unless they opt in via
/// `if: always()`), and marks the run terminal once every job has reached a terminal state.
/// Every database touch goes through `run_client` (an `RcpRunClient` in real use), never a direct
/// `SqlitePool` — this function has no idea whether it's talking to a local bucket over a named
/// pipe or, once remote agents exist, one on another machine entirely.
#[allow(clippy::too_many_arguments)]
pub async fn run_inner(
    run_client: &Arc<dyn RunClient>,
    docker: &Option<Docker>,
    workspaces_dir: &Path,
    buckets_dir: &Path,
    artifacts_dir: &Path,
    bucket_id: &str,
    shell_id: &str,
    workflow_run_id: &str,
    workflow: &Workflow,
    checkout: Option<CheckoutContext>,
    trigger_event: &str,
    max_concurrent_jobs: usize,
    job_runs: Vec<(String, String)>, // (job_key, job_run_id), pre-resolved by the caller
) -> Result<bool> {
    run_client.set_run_status(workflow_run_id, "running", false).await?;

    let job_run_ids: HashMap<String, String> = job_runs.into_iter().collect();

    let semaphore = Arc::new(Semaphore::new(max_concurrent_jobs.max(1)));
    let mut results: HashMap<String, bool> = HashMap::new();
    let mut in_flight: HashSet<String> = HashSet::new();
    let mut handles = Vec::new();

    loop {
        let pending: Vec<String> =
            job_run_ids.keys().filter(|k| !results.contains_key(*k) && !in_flight.contains(*k)).cloned().collect();

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
            let should_run = job.if_condition.as_deref().map(|expr| evaluate(expr, &ctx)).unwrap_or(!any_failed_dep);

            let job_run_id = job_run_ids.get(&job_key).cloned().unwrap();

            if !should_run {
                run_client.set_job_status(&job_run_id, "skipped", None, true).await?;
                results.insert(job_key.clone(), false);
                dispatched_any = true;
                continue;
            }

            dispatched_any = true;
            let permit = semaphore.clone().acquire_owned().await?;
            let run_client = run_client.clone();
            let docker = docker.clone();
            let workflow_run_id = workflow_run_id.to_string();
            let workspaces_dir = workspaces_dir.to_path_buf();
            let buckets_dir = buckets_dir.to_path_buf();
            let artifacts_dir = artifacts_dir.to_path_buf();
            let bucket_id = bucket_id.to_string();
            let shell_id = shell_id.to_string();
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
                let outcome = executor::run_job(
                    &run_client,
                    &docker,
                    &workspaces_dir,
                    &buckets_dir,
                    &artifacts_dir,
                    &bucket_id,
                    &shell_id,
                    &workflow_run_id,
                    &job_run_id,
                    &job,
                    checkout,
                )
                .await
                .unwrap_or(false);
                (job_key_owned, outcome)
            });
            handles.push(handle);
        }

        if !dispatched_any && !handles.is_empty() {
            let (result, _index, remaining) = futures::future::select_all(handles).await;
            handles = remaining;
            if let Ok((job_key, outcome)) = result {
                results.insert(job_key, outcome);
            }
        } else if !dispatched_any && handles.is_empty() {
            break;
        }
    }

    for handle in handles {
        if let Ok((job_key, outcome)) = handle.await {
            results.insert(job_key, outcome);
        }
    }

    let all_succeeded = results.values().all(|ok| *ok);
    run_client.set_run_status(workflow_run_id, if all_succeeded { "succeeded" } else { "failed" }, true).await?;

    for job_run_id in job_run_ids.values() {
        crate::runner::workspace::cleanup(workspaces_dir, job_run_id);
    }

    Ok(all_succeeded)
}
