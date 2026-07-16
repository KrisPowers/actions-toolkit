use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::{Repo, RepoPublic, WebhookEvent};
use crate::db::queries::{repos as repo_queries, webhook_events as event_queries};
use crate::error::{AppError, AppResult};
use crate::github::webhook_verify::generate_secret;

fn to_public(repo: &Repo) -> RepoPublic {
    RepoPublic {
        id: repo.id.clone(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        default_branch: repo.default_branch.clone(),
        webhook_url: format!("/webhooks/github/{}", repo.id),
        created_at: repo.created_at.clone(),
        updated_at: repo.updated_at.clone(),
    }
}

pub async fn list(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Vec<RepoPublic>>> {
    let repos = repo_queries::list(&state.db).await?;
    Ok(Json(repos.iter().map(to_public).collect()))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<RepoPublic>> {
    let repo = repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_public(&repo)))
}

#[derive(Deserialize)]
pub struct CreateRepoRequest {
    pub owner: String,
    pub name: String,
    pub default_branch: Option<String>,
}

#[derive(Serialize)]
pub struct CreateRepoResponse {
    #[serde(flatten)]
    pub repo: RepoPublic,
    pub webhook_secret: String,
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateRepoRequest>,
) -> AppResult<Json<CreateRepoResponse>> {
    if req.owner.trim().is_empty() || req.name.trim().is_empty() {
        return Err(AppError::BadRequest("owner and name are required".into()));
    }

    let webhook_secret = generate_secret();
    let (secret_encrypted, secret_nonce) =
        state.enc.encrypt_str(&webhook_secret).map_err(AppError::Internal)?;

    let repo = repo_queries::create(
        &state.db,
        req.owner.trim(),
        req.name.trim(),
        req.default_branch.as_deref().unwrap_or("main"),
        &secret_encrypted,
        &secret_nonce,
        &user.id,
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            AppError::Conflict("repo already connected".into())
        }
        other => AppError::Database(other),
    })?;

    Ok(Json(CreateRepoResponse {
        repo: to_public(&repo),
        webhook_secret,
    }))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<()> {
    repo_queries::delete(&state.db, &id).await?;
    Ok(())
}

#[derive(Serialize)]
pub struct TestConnectionResponse {
    pub ok: bool,
    pub message: String,
}

pub async fn webhook_events(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<WebhookEvent>>> {
    Ok(Json(event_queries::list_for_repo(&state.db, &id, 100).await?))
}

pub async fn test_connection(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<TestConnectionResponse>> {
    let repo = repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let client = crate::github::client::shared(&state).await?;

    match client.repos(&repo.owner, &repo.name).get().await {
        Ok(_) => Ok(Json(TestConnectionResponse {
            ok: true,
            message: "connection successful".to_string(),
        })),
        Err(e) => Ok(Json(TestConnectionResponse {
            ok: false,
            message: e.to_string(),
        })),
    }
}
