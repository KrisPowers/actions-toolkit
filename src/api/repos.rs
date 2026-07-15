use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::crypto::mask_secret;
use crate::db::models::{Repo, RepoPublic, WebhookEvent};
use crate::db::queries::{repos as repo_queries, webhook_events as event_queries};
use crate::error::{AppError, AppResult};
use crate::github::webhook_verify::generate_secret;

fn to_public(repo: &Repo, state: &AppState) -> AppResult<RepoPublic> {
    let pat = state
        .enc
        .decrypt_str(&repo.pat_encrypted, &repo.pat_nonce)
        .map_err(AppError::Internal)?;
    Ok(RepoPublic {
        id: repo.id.clone(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        default_branch: repo.default_branch.clone(),
        pat_masked: mask_secret(&pat),
        webhook_url: format!("/webhooks/github/{}", repo.id),
        created_at: repo.created_at.clone(),
        updated_at: repo.updated_at.clone(),
    })
}

pub async fn list(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Vec<RepoPublic>>> {
    let repos = repo_queries::list(&state.db).await?;
    let out: AppResult<Vec<RepoPublic>> = repos.iter().map(|r| to_public(r, &state)).collect();
    Ok(Json(out?))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<RepoPublic>> {
    let repo = repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_public(&repo, &state)?))
}

#[derive(Deserialize)]
pub struct CreateRepoRequest {
    pub owner: String,
    pub name: String,
    pub default_branch: Option<String>,
    pub pat: String,
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
    if req.owner.trim().is_empty() || req.name.trim().is_empty() || req.pat.trim().is_empty() {
        return Err(AppError::BadRequest("owner, name, and pat are required".into()));
    }

    let (pat_encrypted, pat_nonce) = state.enc.encrypt_str(&req.pat).map_err(AppError::Internal)?;
    let webhook_secret = generate_secret();
    let (secret_encrypted, secret_nonce) =
        state.enc.encrypt_str(&webhook_secret).map_err(AppError::Internal)?;

    let repo = repo_queries::create(
        &state.db,
        req.owner.trim(),
        req.name.trim(),
        req.default_branch.as_deref().unwrap_or("main"),
        &pat_encrypted,
        &pat_nonce,
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
        repo: to_public(&repo, &state)?,
        webhook_secret,
    }))
}

#[derive(Deserialize)]
pub struct UpdatePatRequest {
    pub pat: String,
}

pub async fn update_pat(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
    Json(req): Json<UpdatePatRequest>,
) -> AppResult<Json<RepoPublic>> {
    repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let (pat_encrypted, pat_nonce) = state.enc.encrypt_str(&req.pat).map_err(AppError::Internal)?;
    repo_queries::update_pat(&state.db, &id, &pat_encrypted, &pat_nonce).await?;
    crate::github::client::invalidate(&state, &id);
    let repo = repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(to_public(&repo, &state)?))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<()> {
    repo_queries::delete(&state.db, &id).await?;
    crate::github::client::invalidate(&state, &id);
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
    let client = crate::github::client::for_repo(&state, &repo).await.map_err(AppError::Internal)?;

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
