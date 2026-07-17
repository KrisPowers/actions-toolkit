use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::Workflow as WorkflowRow;
use crate::db::queries::repos as repo_queries;
use crate::db::queries::workflows as workflow_queries;
use crate::error::{AppError, AppResult};
use crate::github::{actions, client, issues, releases};
use crate::workflow::{validate, yaml};

async fn client_for(state: &AppState, repo_id: &str) -> AppResult<(octocrab::Octocrab, crate::db::models::Repo)> {
    let repo = repo_queries::find_by_id(&state.db, repo_id).await?.ok_or(AppError::NotFound)?;
    let client = client::shared(state).await?;
    Ok((client, repo))
}

#[derive(Deserialize)]
pub struct StateQuery {
    state: Option<String>,
}

pub async fn list_issues(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Query(q): Query<StateQuery>,
    _user: CurrentUser,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = issues::list_issues(&client, &repo.owner, &repo.name, q.state.as_deref().unwrap_or("open"))
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

pub async fn get_issue(
    State(state): State<AppState>,
    Path((repo_id, number)): Path<(String, u64)>,
    _user: CurrentUser,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = issues::get_issue(&client, &repo.owner, &repo.name, number).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

#[derive(Deserialize)]
pub struct CommentRequest {
    pub body: String,
}

pub async fn add_comment(
    State(state): State<AppState>,
    Path((repo_id, number)): Path<(String, u64)>,
    _user: CurrentUser,
    Json(req): Json<CommentRequest>,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = issues::add_comment(&client, &repo.owner, &repo.name, number, &req.body)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

#[derive(Deserialize)]
pub struct UpdateIssueRequest {
    pub state: Option<String>,
    pub add_labels: Option<Vec<String>>,
    pub remove_label: Option<String>,
}

pub async fn update_issue(
    State(state): State<AppState>,
    Path((repo_id, number)): Path<(String, u64)>,
    _user: CurrentUser,
    Json(req): Json<UpdateIssueRequest>,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;

    if let Some(labels) = &req.add_labels {
        issues::add_labels(&client, &repo.owner, &repo.name, number, labels)
            .await
            .map_err(AppError::Internal)?;
    }
    if let Some(label) = &req.remove_label {
        issues::remove_label(&client, &repo.owner, &repo.name, number, label)
            .await
            .map_err(AppError::Internal)?;
    }

    let updated = match req.state.as_deref() {
        Some("closed") => issues::close_issue(&client, &repo.owner, &repo.name, number).await,
        Some("open") => issues::reopen_issue(&client, &repo.owner, &repo.name, number).await,
        _ => issues::get_issue(&client, &repo.owner, &repo.name, number).await,
    }
    .map_err(AppError::Internal)?;

    Ok(Json(serde_json::to_value(updated).map_err(|e| AppError::Internal(e.into()))?))
}

pub async fn list_pull_requests(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Query(q): Query<StateQuery>,
    _user: CurrentUser,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = issues::list_pull_requests(&client, &repo.owner, &repo.name, q.state.as_deref().unwrap_or("open"))
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

pub async fn get_pull_request(
    State(state): State<AppState>,
    Path((repo_id, number)): Path<(String, u64)>,
    _user: CurrentUser,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = issues::get_pull_request(&client, &repo.owner, &repo.name, number)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

pub async fn list_releases(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = releases::list_releases(&client, &repo.owner, &repo.name).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

pub async fn get_release(
    State(state): State<AppState>,
    Path((repo_id, release_id)): Path<(String, u64)>,
    _user: CurrentUser,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = releases::get_release(&client, &repo.owner, &repo.name, release_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

#[derive(Deserialize)]
pub struct CreateReleaseRequest {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

pub async fn create_release(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
    Json(req): Json<CreateReleaseRequest>,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = releases::create_release(
        &client,
        &repo.owner,
        &repo.name,
        releases::CreateReleaseParams {
            tag_name: &req.tag_name,
            name: req.name.as_deref(),
            body: req.body.as_deref(),
            draft: req.draft,
            prerelease: req.prerelease,
        },
    )
    .await
    .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}

/// The workflow files GitHub Actions itself would run for this repo (under
/// `.github/workflows`), distinct from the workflows this app runs locally.
pub async fn list_github_workflows(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<actions::GithubWorkflowFile>>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = actions::list_workflow_files(&client, &repo.owner, &repo.name).await.map_err(AppError::Internal)?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct ImportGithubWorkflowRequest {
    pub path: String,
}

/// Pull a GitHub Actions workflow file's YAML and register it as a workflow this app runs
/// locally. Fails with a 400 if the file uses features this app's runner doesn't understand.
pub async fn import_github_workflow(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
    Json(req): Json<ImportGithubWorkflowRequest>,
) -> AppResult<Json<WorkflowRow>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let yaml_source = actions::get_workflow_content(&client, &repo.owner, &repo.name, &req.path)
        .await
        .map_err(AppError::Internal)?;

    let model = yaml::parse(&yaml_source).map_err(|e| AppError::BadRequest(e.to_string()))?;
    validate::validate(&model).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let canonical_yaml = yaml::to_yaml(&model).map_err(AppError::Internal)?;
    let canonical_json = yaml::to_json(&model).map_err(AppError::Internal)?;

    let name = if model.name.trim().is_empty() {
        req.path.rsplit('/').next().unwrap_or(&req.path).to_string()
    } else {
        model.name.clone()
    };
    let description = format!("Imported from GitHub Actions ({})", req.path);

    let workflow = workflow_queries::create(
        &state.db,
        &repo_id,
        &name,
        Some(&description),
        &req.path,
        &canonical_yaml,
        &canonical_json,
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            AppError::Conflict("a workflow with this name already exists for this repo".into())
        }
        other => AppError::Database(other),
    })?;

    Ok(Json(workflow))
}

#[derive(Deserialize)]
pub struct UpdateReleaseRequest {
    pub name: Option<String>,
    pub body: Option<String>,
    pub draft: Option<bool>,
    pub prerelease: Option<bool>,
}

pub async fn update_release(
    State(state): State<AppState>,
    Path((repo_id, release_id)): Path<(String, u64)>,
    _user: CurrentUser,
    Json(req): Json<UpdateReleaseRequest>,
) -> AppResult<Json<Value>> {
    let (client, repo) = client_for(&state, &repo_id).await?;
    let result = releases::update_release(
        &client,
        &repo.owner,
        &repo.name,
        release_id,
        req.name.as_deref(),
        req.body.as_deref(),
        req.draft,
        req.prerelease,
    )
    .await
    .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(result).map_err(|e| AppError::Internal(e.into()))?))
}
