use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::queries::repos as repo_queries;
use crate::error::{AppError, AppResult};
use crate::github::{client, issues, releases};

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
