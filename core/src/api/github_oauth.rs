use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::config::GITHUB_APP_SLUG;
use crate::db::queries::github_token as token_queries;
use crate::error::{AppError, AppResult};
use crate::github::{client, discovery, oauth};

#[derive(Serialize)]
pub struct DeviceStartResponse {
    pub user_code: String,
    pub verification_uri: String,
    pub interval: i64,
    pub expires_in: i64,
}

/// Starts a device-flow connect attempt. Gated behind `CurrentUser` so only a logged-in operator
/// can kick off a connect that would replace the instance-wide GitHub connection.
pub async fn device_start(State(state): State<AppState>, CurrentUser(_user): CurrentUser) -> AppResult<Json<DeviceStartResponse>> {
    let started =
        oauth::start_device_flow(&state.config.github_device_code_url, &state.config.github_app_client_id).await.map_err(AppError::Internal)?;

    *state.pending_device_flow.write().await = Some(oauth::PendingDeviceFlow {
        device_code: started.device_code,
        interval_secs: started.interval,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(started.expires_in),
    });

    Ok(Json(DeviceStartResponse {
        user_code: started.user_code,
        verification_uri: started.verification_uri,
        interval: started.interval,
        expires_in: started.expires_in,
    }))
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePollResponse {
    Pending,
    Denied,
    Expired,
    /// No connect attempt is in progress (nothing to poll, or it already finished/expired and
    /// was cleared), distinct from `Expired` so the frontend doesn't show a stale "expired"
    /// message for a poll that arrives after the flow already completed successfully.
    NotStarted,
    Connected {
        github_login: String,
        has_installation: bool,
    },
}

/// Polls once for whether the in-flight device-flow attempt has been approved. The frontend
/// calls this on a timer at the interval `device_start` returned. Any terminal outcome (denied,
/// expired, connected, or an unexpected error) clears the pending attempt so a stale poll can't
/// resurrect it.
pub async fn device_poll(State(state): State<AppState>, CurrentUser(_user): CurrentUser) -> AppResult<Json<DevicePollResponse>> {
    let Some(pending) = state.pending_device_flow.read().await.clone() else {
        return Ok(Json(DevicePollResponse::NotStarted));
    };

    if chrono::Utc::now() > pending.expires_at {
        *state.pending_device_flow.write().await = None;
        return Ok(Json(DevicePollResponse::Expired));
    }

    match oauth::poll_device_token(&state.config.github_oauth_token_url, &state.config.github_app_client_id, &pending.device_code).await {
        Ok(oauth::DevicePollOutcome::Pending) => Ok(Json(DevicePollResponse::Pending)),
        Ok(oauth::DevicePollOutcome::SlowDown { new_interval_secs }) => {
            if let Some(p) = state.pending_device_flow.write().await.as_mut() {
                p.interval_secs = new_interval_secs;
            }
            Ok(Json(DevicePollResponse::Pending))
        }
        Ok(oauth::DevicePollOutcome::Denied) => {
            *state.pending_device_flow.write().await = None;
            Ok(Json(DevicePollResponse::Denied))
        }
        Ok(oauth::DevicePollOutcome::Expired) => {
            *state.pending_device_flow.write().await = None;
            Ok(Json(DevicePollResponse::Expired))
        }
        Ok(oauth::DevicePollOutcome::Success(exchanged)) => {
            *state.pending_device_flow.write().await = None;
            persist_connection(&state, exchanged).await.map(Json)
        }
        Err(e) => {
            *state.pending_device_flow.write().await = None;
            Err(AppError::Internal(e))
        }
    }
}

async fn persist_connection(state: &AppState, exchanged: oauth::ExchangedToken) -> AppResult<DevicePollResponse> {
    let github_client = client::for_token(&exchanged.access_token).map_err(AppError::Internal)?;
    let login = discovery::validate_token(&github_client).await.map_err(AppError::Internal)?;
    let installation_id = discovery::find_installation_id(&github_client, GITHUB_APP_SLUG).await.map_err(AppError::Internal)?;

    let (token_encrypted, token_nonce) = state.enc.encrypt_str(&exchanged.access_token).map_err(AppError::Internal)?;
    let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str(&exchanged.refresh_token).map_err(AppError::Internal)?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(exchanged.expires_in)).to_rfc3339();

    token_queries::upsert_app_token(&state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &expires_at, installation_id, &login)
        .await?;
    client::invalidate(state).await;

    Ok(DevicePollResponse::Connected { github_login: login, has_installation: installation_id.is_some() })
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let data_dir = std::env::temp_dir().join(format!("atk-oauth-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let db = crate::db::connect(&data_dir.join("db.sqlite")).await.unwrap();
        let enc = EncryptionKey::load_or_generate(None, &data_dir.join("secrets")).unwrap();
        let config = AppConfig {
            data_dir,
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: mock_server.uri(),
            github_device_code_url: oauth::GITHUB_DEVICE_CODE_URL.to_string(),
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
            token_refresh_lock: tokio::sync::Mutex::new(()),
        }));

        (state, user)
    }

    fn pending(expires_in: chrono::Duration) -> oauth::PendingDeviceFlow {
        oauth::PendingDeviceFlow {
            device_code: "test-device-code".to_string(),
            interval_secs: 5,
            expires_at: chrono::Utc::now() + expires_in,
        }
    }

    /// Rule-proving test: GitHub reporting "authorization_pending" must not clear the in-flight
    /// attempt (the operator hasn't answered yet, there's nothing to restart) and must not store
    /// any token.
    #[tokio::test]
    async fn poll_reports_pending_and_keeps_the_attempt() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "authorization_pending" })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Pending));
        assert!(state.pending_device_flow.read().await.is_some(), "a pending attempt must survive an authorization_pending poll");
        assert!(token_queries::get(&state.db).await.unwrap().is_none(), "no token should be stored while still pending");
    }

    /// Rule-proving test: an explicit decline on GitHub's side must clear the attempt (so a stale
    /// poll can't resurrect it) and must not store any token.
    #[tokio::test]
    async fn poll_clears_the_attempt_and_reports_denied() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "access_denied" })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Denied));
        assert!(state.pending_device_flow.read().await.is_none(), "a denied attempt must be cleared, not left for a stale poll to resurrect");
        assert!(token_queries::get(&state.db).await.unwrap().is_none());
    }

    /// Rule-proving test: GitHub reporting the device code itself expired must clear the attempt
    /// and must not store any token.
    #[tokio::test]
    async fn poll_clears_the_attempt_and_reports_expired_from_github() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "expired_token" })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Expired));
        assert!(state.pending_device_flow.read().await.is_none());
        assert!(token_queries::get(&state.db).await.unwrap().is_none());
    }

    /// Rule-proving test: once the locally-tracked deadline has passed, `device_poll` must report
    /// `Expired` and clear the attempt *without* ever calling GitHub — proven here by mounting no
    /// mock at all, so any HTTP call would fail the request rather than silently succeed.
    #[tokio::test]
    async fn poll_reports_expired_locally_without_calling_github() {
        let mock_server = MockServer::start().await;
        // Deliberately no `Mock::given(...).mount(...)` — any request to this server 404s.

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::seconds(-5)));

        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Expired));
        assert!(state.pending_device_flow.read().await.is_none());
    }

    /// Rule-proving test: polling with no connect attempt in progress (never started, or already
    /// resolved and cleared) reports `NotStarted` rather than a stale terminal state.
    #[tokio::test]
    async fn poll_reports_not_started_when_nothing_is_pending() {
        let mock_server = MockServer::start().await;
        let (state, user) = test_state(&mock_server).await;

        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::NotStarted));
    }

    /// Rule-proving test for the milestone's "no raw token ever reaches the frontend" rule:
    /// serializing `GithubTokenStatus` must produce exactly this field set, nothing more. A
    /// future field addition (e.g. a raw token, by mistake) changes this test's expected key set
    /// and forces a conscious decision, instead of silently reaching the frontend unnoticed.
    #[test]
    fn github_token_status_serializes_no_more_than_the_known_safe_fields() {
        let status = crate::db::models::GithubTokenStatus {
            connected: true,
            github_login: Some("octocat".to_string()),
            scopes: Some("repo".to_string()),
            connected_at: Some("2020-01-01T00:00:00Z".to_string()),
            token_type: Some("github_app".to_string()),
            needs_reconnect: false,
        };
        let value = serde_json::to_value(&status).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["connected", "connected_at", "github_login", "needs_reconnect", "scopes", "token_type"]);
    }

    /// Rule-proving test for the milestone's "tokens are unreadable directly in the database"
    /// rule: the encrypted bytes stored for a token must not contain the plaintext anywhere, and
    /// must only be recoverable by decrypting through `state.enc` — not by, say, reading the
    /// database file directly with a hex editor or `strings`.
    #[tokio::test]
    async fn stored_tokens_are_unreadable_without_the_encryption_key() {
        let mock_server = MockServer::start().await;
        let (state, _user) = test_state(&mock_server).await;

        let plaintext_access = "gho_supersecretaccesstoken1234567890";
        let plaintext_refresh = "ghr_supersecretrefreshtoken1234567890";
        let (token_encrypted, token_nonce) = state.enc.encrypt_str(plaintext_access).unwrap();
        let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str(plaintext_refresh).unwrap();
        let expires_at = chrono::Utc::now().to_rfc3339();

        token_queries::upsert_app_token(&state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &expires_at, None, "octocat")
            .await
            .unwrap();

        // Read the raw stored bytes back out, the same way anyone with direct file access to the
        // SQLite database would see them -- not through any app-level decrypt path.
        let stored = token_queries::get(&state.db).await.unwrap().unwrap();
        assert_ne!(stored.token_encrypted, plaintext_access.as_bytes(), "the stored bytes must not be the plaintext token");
        assert!(
            !String::from_utf8_lossy(&stored.token_encrypted).contains(plaintext_access),
            "the plaintext token must not appear anywhere in the stored ciphertext"
        );
        assert!(!String::from_utf8_lossy(&stored.refresh_token_encrypted.clone().unwrap()).contains(plaintext_refresh));

        // Only decryptable through state.enc, and round-trips to the exact original.
        let decrypted = state.enc.decrypt_str(&stored.token_encrypted, &stored.token_nonce).unwrap();
        assert_eq!(decrypted, plaintext_access);
    }
}
