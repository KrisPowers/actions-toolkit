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
        pat,
        git_ref: ref_name.map(str::to_string).unwrap_or_else(|| format!("refs/heads/{}", repo.default_branch)),
    });

    let state_arc = Arc::new(state.clone());
    let run_id = run.id.clone();
    let event = trigger_event.to_string();
    tokio::spawn(crate::runner::scheduler::run(state_arc, run_id, model, checkout, event));

    Ok(run)
}
