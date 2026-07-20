use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::{Repo, RepoPublic, WebhookEvent};
use crate::db::queries::{repos as repo_queries, webhook_events as event_queries};
use crate::error::{AppError, AppResult};
use crate::github::webhook_verify::generate_secret;
use crate::github::{client, hooks};

fn to_public(repo: &Repo) -> RepoPublic {
    RepoPublic {
        id: repo.id.clone(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        default_branch: repo.default_branch.clone(),
        webhook_url: format!("/webhooks/github/{}", repo.id),
        webhook_connected: repo.github_hook_id.is_some(),
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
}

pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    headers: HeaderMap,
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

    // The webhook is created against the just-inserted repo (its payload URL is keyed by repo
    // id), so on failure the row is rolled back rather than left half-connected with no working
    // trigger path.
    let github_client = client::shared(&state).await?;
    let payload_url = format!("{}/webhooks/github/{}", crate::api::request_origin(&headers), repo.id);
    match hooks::create_webhook(&github_client, &repo.owner, &repo.name, &payload_url, &webhook_secret).await {
        Ok(hook_id) => repo_queries::set_github_hook_id(&state.db, &repo.id, hook_id as i64).await?,
        Err(e) => {
            let _ = repo_queries::delete(&state.db, &repo.id).await;
            return Err(AppError::BadRequest(format!("connected the repo but failed to create its GitHub webhook: {e}")));
        }
    }

    let repo = repo_queries::find_by_id(&state.db, &repo.id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(CreateRepoResponse { repo: to_public(&repo) }))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _user: CurrentUser,
) -> AppResult<()> {
    let repo = repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;

    // Best-effort: a transient GitHub-side failure here shouldn't strand the repo as "still
    // connected" locally when the operator's clear intent was to disconnect it. A 404 (already
    // gone) is handled as success inside delete_webhook itself, not here.
    if let Some(hook_id) = repo.github_hook_id {
        match client::shared(&state).await {
            Ok(github_client) => {
                if let Err(e) = hooks::delete_webhook(&github_client, &repo.owner, &repo.name, hook_id as u64).await {
                    tracing::warn!(error = %e, repo_id = %id, "failed to delete the GitHub webhook while disconnecting; removing the local row anyway");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, repo_id = %id, "no working GitHub connection while disconnecting; leaving the webhook in place on GitHub's side");
            }
        }
    }

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

#[derive(serde::Serialize)]
pub struct SyncResponse {
    /// Whether a new release was found and dispatched. `false` just means nothing new since the
    /// last sync, not that anything went wrong.
    pub dispatched: bool,
}

/// Manual trigger for the polling fallback (`runner::poll_sync`) — lets an operator sync
/// immediately instead of waiting for the periodic sweep, e.g. right after publishing a release
/// on a repo without a working webhook.
pub async fn sync(State(state): State<AppState>, Path(id): Path<String>, _user: CurrentUser) -> AppResult<Json<SyncResponse>> {
    let repo = repo_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    let dispatched = crate::runner::poll_sync::sync_repo_releases(&state, &repo).await.map_err(AppError::Internal)?;
    Ok(Json(SyncResponse { dispatched }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo connected before webhook automation existed (or whose creation left the row behind
    /// despite a failed hook) has `github_hook_id: None`; the API must report that as not
    /// connected rather than silently claiming the trigger path works.
    #[test]
    fn to_public_reports_webhook_connected_false_when_hook_id_is_none() {
        let repo = Repo {
            id: "repo-1".to_string(),
            owner: "octocat".to_string(),
            name: "hello-world".to_string(),
            default_branch: "main".to_string(),
            webhook_secret_encrypted: vec![],
            webhook_secret_nonce: vec![],
            created_by: "user-1".to_string(),
            created_at: "2020-01-01T00:00:00Z".to_string(),
            updated_at: "2020-01-01T00:00:00Z".to_string(),
            github_hook_id: None,
            last_synced_release_id: None,
        };
        assert!(!to_public(&repo).webhook_connected);
    }

    use crate::app::{AppState, AppStateInner};
    use crate::auth::jwt::JwtCodec;
    use crate::config::AppConfig;
    use crate::crypto::EncryptionKey;
    use crate::db::models::User;
    use crate::db::queries::users as user_queries;
    use crate::runner::log_stream::LogHub;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_state(mock_server: &MockServer) -> (AppState, User) {
        let test_id = uuid::Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-repos-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let db = crate::db::connect(&data_dir.join("db.sqlite")).await.unwrap();
        let enc = EncryptionKey::load_or_generate(None, &data_dir.join("secrets")).unwrap();
        let config = AppConfig {
            data_dir,
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: crate::github::oauth::GITHUB_TOKEN_URL.to_string(),
            github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
        };
        let user = user_queries::create(&db, "tester", "hash", "admin").await.unwrap();

        let state = AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt: JwtCodec::new("test-secret"),
            enc,
            docker: None,
            bucket_capability_ok: true,
            bucket_capability_reason: None,
            log_hub: Arc::new(LogHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
        }));

        // Pre-seed the cached client pointed at the mock server, since client::shared() would
        // otherwise try to decrypt a (nonexistent) stored token and reach the real GitHub API.
        let github_client = octocrab::Octocrab::builder().base_uri(mock_server.uri()).unwrap().personal_token("test-token".to_string()).build().unwrap();
        *state.github_client.write().await = Some(github_client);

        (state, user)
    }

    fn test_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(axum::http::header::HOST, "example.com".parse().unwrap());
        headers
    }

    fn connect_request() -> CreateRepoRequest {
        CreateRepoRequest { owner: "octocat".to_string(), name: "hello-world".to_string(), default_branch: None }
    }

    /// Rule-proving test: a repo that fails to get its GitHub webhook created is not left behind
    /// half-connected with no working trigger path.
    #[tokio::test]
    async fn create_rolls_back_the_repo_row_when_webhook_creation_fails() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/hooks"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        let result = create(State(state.clone()), CurrentUser(user), test_headers(), Json(connect_request())).await;
        assert!(result.is_err());
        assert!(repo_queries::list(&state.db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_stores_the_hook_id_returned_by_github() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/hooks"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": 555 })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        let response = create(State(state.clone()), CurrentUser(user), test_headers(), Json(connect_request())).await.unwrap();

        let repo = repo_queries::find_by_id(&state.db, &response.0.repo.id).await.unwrap().unwrap();
        assert_eq!(repo.github_hook_id, Some(555));
        assert!(response.0.repo.webhook_connected, "the API response must reflect the stored hook id, not just the DB row");
    }

    /// Rule-proving test: disconnecting a repo removes the corresponding webhook from GitHub, not
    /// just the local row.
    #[tokio::test]
    async fn delete_calls_github_to_remove_the_webhook() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/hooks"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": 777 })))
            .mount(&mock_server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/repos/octocat/hello-world/hooks/777"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        let response = create(State(state.clone()), CurrentUser(user.clone()), test_headers(), Json(connect_request())).await.unwrap();
        let repo_id = response.0.repo.id.clone();

        delete(State(state.clone()), Path(repo_id.clone()), CurrentUser(user)).await.unwrap();

        assert!(repo_queries::find_by_id(&state.db, &repo_id).await.unwrap().is_none());
        let requests = mock_server.received_requests().await.unwrap();
        assert!(requests.iter().any(|r| r.method.as_str() == "DELETE" && r.url.path() == "/repos/octocat/hello-world/hooks/777"));
    }
}
