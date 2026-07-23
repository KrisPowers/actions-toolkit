use std::sync::Arc;

use anyhow::Result;

use crate::app::AppState;
use crate::db::models::{Repo, WorkflowRun, Workflow as WorkflowRow};
use crate::db::queries::runs as run_queries;
use crate::runner::executor::CheckoutContext;
use crate::workflow::{model::Workflow, yaml};

/// Create a `workflow_runs` row (+ one `job_runs` row per job) and spawn the scheduler in the
/// background. Shared by manual dispatch, rerun, and webhook-triggered runs so all three paths
/// stay consistent.
#[allow(clippy::too_many_arguments)]
pub async fn spawn_run(
    state: &AppState,
    workflow_row: &WorkflowRow,
    repo: &Repo,
    trigger_event: &str,
    trigger_payload_json: Option<&str>,
    ref_name: Option<&str>,
    commit_sha: Option<&str>,
    webhook_event_id: Option<&str>,
) -> Result<WorkflowRun> {
    let model: Workflow = yaml::parse(&workflow_row.yaml_source)?;

    let run = run_queries::create_run(
        &state.db,
        &workflow_row.id,
        &repo.id,
        trigger_event,
        trigger_payload_json,
        ref_name,
        commit_sha,
        webhook_event_id,
    )
    .await?;

    for (job_key, job) in &model.jobs {
        let needs_json = serde_json::to_string(&job.needs).unwrap_or_else(|_| "[]".to_string());
        run_queries::create_job_run(&state.db, &run.id, job_key, job.name.as_deref(), &needs_json).await?;
    }

    let pat = crate::github::client::decrypted_token(state).await?;
    let checkout = Some(CheckoutContext {
        owner: repo.owner.clone(),
        repo: repo.name.clone(),
        repo_id: repo.id.clone(),
        pat,
        git_ref: ref_name.map(str::to_string).unwrap_or_else(|| format!("refs/heads/{}", repo.default_branch)),
    });

    // Best-effort: a commit-triggered run reports its status back to that commit on GitHub (the
    // same place a real GitHub Actions run would show a check), so pushing to a connected repo
    // shows up there too, not just in this instance's own UI. Skipped for runs with no specific
    // commit (e.g. a manual "Run now" against no particular ref) since there'd be nothing to post
    // it against; failures here are logged, not fatal, since the run itself already succeeded or
    // failed independent of whether GitHub could be told about it.
    // Best-effort GitHub Check Run: this is what actually renders the check mark/X/yellow-spinner
    // GitHub shows next to a commit and in a PR's checks list, real GitHub Actions' own UI for
    // exactly this. Kept alongside the plainer commit status above (older, simpler, still what
    // some external tooling and branch-protection setups key off), not a replacement for it.
    let mut check_run_id = None;
    if let Some(sha) = commit_sha {
        let target_url = crate::runner::github_status::run_target_url(state, &run.id).await;
        match crate::runner::github_status::start_check(state, &repo.owner, &repo.name, sha, target_url.clone()).await {
            Ok(id) => check_run_id = Some(id),
            Err(e) => tracing::warn!(error = format!("{e:#}"), repo = %repo.id, sha, "failed to start the GitHub check run"),
        }
        if let Err(e) = crate::runner::github_status::report_pending(state, &repo.owner, &repo.name, sha, target_url).await {
            tracing::warn!(error = format!("{e:#}"), repo = %repo.id, sha, "failed to post the pending GitHub commit status");
        }
    }

    let state_arc = Arc::new(state.clone());
    let run_id = run.id.clone();
    let event = trigger_event.to_string();
    let commit_sha = commit_sha.map(str::to_string);
    let owner = repo.owner.clone();
    let name = repo.name.clone();
    tokio::spawn(crate::runner::scheduler::run(state_arc, run_id, model, checkout, event, owner, name, commit_sha, check_run_id));

    Ok(run)
}
