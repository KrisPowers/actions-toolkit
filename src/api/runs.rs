use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::{RunLog, RunTree, WorkflowRun};
use crate::db::queries::{repos as repo_queries, runs as run_queries, workflows as workflow_queries};
use crate::error::{AppError, AppResult};

#[derive(Deserialize)]
pub struct ListRunsQuery {
    limit: Option<i64>,
}

pub async fn list_for_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Query(q): Query<ListRunsQuery>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<WorkflowRun>>> {
    let runs = run_queries::list_runs_for_repo(&state.db, &repo_id, q.limit.unwrap_or(50)).await?;
    Ok(Json(runs))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<RunTree>> {
    let tree = run_queries::run_tree(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(tree))
}

#[derive(Deserialize)]
pub struct LogsQuery {
    since_id: Option<i64>,
    step_run_id: Option<String>,
}

pub async fn logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<LogsQuery>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<RunLog>>> {
    let since = q.since_id.unwrap_or(0);
    let logs = match q.step_run_id {
        Some(step_id) => run_queries::list_logs_for_step(&state.db, &step_id, since).await?,
        None => run_queries::list_logs_for_run(&state.db, &id, since).await?,
    };
    Ok(Json(logs))
}

pub async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<()> {
    let run = run_queries::find_run(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    if matches!(run.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Err(AppError::Conflict("run has already finished".into()));
    }

    if let Some(docker) = &state.docker {
        let containers = crate::runner::docker::list_labeled_containers(docker, &id)
            .await
            .map_err(AppError::Internal)?;
        for container_id in containers {
            let _ = crate::runner::docker::remove_container(docker, &container_id).await;
        }
    }

    run_queries::set_run_status(&state.db, &id, "cancelled", true).await?;
    Ok(())
}

pub async fn rerun(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<WorkflowRun>> {
    let previous = run_queries::find_run(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let workflow_row = workflow_queries::find_by_id(&state.db, &previous.workflow_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let repo = repo_queries::find_by_id(&state.db, &workflow_row.repo_id).await?.ok_or(AppError::NotFound)?;

    let run = crate::runner::dispatch::spawn_run(
        &state,
        &workflow_row,
        &repo,
        "rerun",
        previous.trigger_payload_json.as_deref(),
        previous.ref_name.as_deref(),
        previous.commit_sha.as_deref(),
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Json(run))
}
