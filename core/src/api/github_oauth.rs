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
///
/// Also spawns `run_device_flow_poller`, which polls GitHub for this attempt on the server, not
/// just in response to the frontend's own poll requests. If that spawn didn't happen and only the
/// frontend polled, closing the browser tab the instant GitHub says "you're done" would silently
/// lose the connection: GitHub really did approve it, but nothing would ever ask GitHub for the
/// resulting token, so the device code just expires unused and the operator sees no connection.
pub async fn device_start(State(state): State<AppState>, CurrentUser(_user): CurrentUser) -> AppResult<Json<DeviceStartResponse>> {
    let started =
        oauth::start_device_flow(&state.config.github_device_code_url, &state.config.github_app_client_id).await.map_err(AppError::Internal)?;

    let device_code = started.device_code.clone();
    *state.pending_device_flow.write().await = Some(oauth::PendingDeviceFlow {
        device_code: device_code.clone(),
        interval_secs: started.interval,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(started.expires_in),
    });
    *state.device_flow_result.write().await = None;

    tokio::spawn(run_device_flow_poller(state.clone(), device_code, started.interval));

    Ok(Json(DeviceStartResponse {
        user_code: started.user_code,
        verification_uri: started.verification_uri,
        interval: started.interval,
        expires_in: started.expires_in,
    }))
}

/// Polls GitHub for this device-flow attempt's outcome until a terminal one (or the
/// locally-tracked deadline passes), independent of whether any browser tab is still watching.
/// Writes the result to `state.device_flow_result` (and, on success, persists the connection)
/// rather than returning anything, since nothing is waiting on this task directly; `device_poll`
/// picks the result up from there whenever the frontend next asks.
async fn run_device_flow_poller(state: AppState, device_code: String, mut interval_secs: i64) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs.max(1) as u64)).await;

        let expires_at = {
            let pending = state.pending_device_flow.read().await;
            match pending.as_ref() {
                // A newer `device_start` call replaced this attempt; stop polling for a code
                // nobody is tracking anymore instead of clobbering the new attempt's result.
                Some(p) if p.device_code == device_code => p.expires_at,
                _ => return,
            }
        };
        if chrono::Utc::now() > expires_at {
            finish_device_flow(&state, &device_code, oauth::DeviceFlowResult::Expired).await;
            return;
        }

        match oauth::poll_device_token(&state.config.github_oauth_token_url, &state.config.github_app_client_id, &device_code).await {
            Ok(oauth::DevicePollOutcome::Pending) => continue,
            Ok(oauth::DevicePollOutcome::SlowDown { new_interval_secs }) => {
                interval_secs = new_interval_secs;
                if let Some(p) = state.pending_device_flow.write().await.as_mut() {
                    if p.device_code == device_code {
                        p.interval_secs = new_interval_secs;
                    }
                }
            }
            Ok(oauth::DevicePollOutcome::Denied) => {
                finish_device_flow(&state, &device_code, oauth::DeviceFlowResult::Denied).await;
                return;
            }
            Ok(oauth::DevicePollOutcome::Expired) => {
                finish_device_flow(&state, &device_code, oauth::DeviceFlowResult::Expired).await;
                return;
            }
            Ok(oauth::DevicePollOutcome::Success(exchanged)) => {
                let result = match persist_connection(&state, exchanged).await {
                    Ok(DevicePollResponse::Connected { github_login, has_installation }) => {
                        oauth::DeviceFlowResult::Connected { github_login, has_installation }
                    }
                    Ok(_) => unreachable!("persist_connection only ever returns Connected"),
                    Err(e) => {
                        tracing::warn!(error = %e, "device-flow authorization succeeded but persisting the connection failed");
                        oauth::DeviceFlowResult::Failed { message: e.to_string() }
                    }
                };
                finish_device_flow(&state, &device_code, result).await;
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, "device-flow poll request to GitHub failed");
                finish_device_flow(&state, &device_code, oauth::DeviceFlowResult::Failed { message: e.to_string() }).await;
                return;
            }
        }
    }
}

/// Records a terminal outcome and clears the pending attempt, but only if `device_code` is still
/// the one being tracked (see the supersede check in `run_device_flow_poller`'s loop).
async fn finish_device_flow(state: &AppState, device_code: &str, result: oauth::DeviceFlowResult) {
    let mut pending = state.pending_device_flow.write().await;
    if pending.as_ref().map(|p| p.device_code == device_code).unwrap_or(false) {
        *pending = None;
        drop(pending);
        *state.device_flow_result.write().await = Some(result);
    }
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

/// Reports the current state of the in-flight device-flow attempt, if any. The frontend calls
/// this on a timer purely to reflect progress in the UI; the actual polling of GitHub happens
/// server-side in `run_device_flow_poller` (spawned by `device_start`), so this just reads
/// whatever that background task has (or hasn't yet) recorded. That split is what lets the
/// attempt still complete when GitHub reports success even if no browser tab is open to see it.
pub async fn device_poll(State(state): State<AppState>, CurrentUser(_user): CurrentUser) -> AppResult<Json<DevicePollResponse>> {
    if let Some(result) = state.device_flow_result.read().await.clone() {
        return match result {
            oauth::DeviceFlowResult::Denied => Ok(Json(DevicePollResponse::Denied)),
            oauth::DeviceFlowResult::Expired => Ok(Json(DevicePollResponse::Expired)),
            oauth::DeviceFlowResult::Connected { github_login, has_installation } => {
                Ok(Json(DevicePollResponse::Connected { github_login, has_installation }))
            }
            oauth::DeviceFlowResult::Failed { message } => Err(AppError::Internal(anyhow::anyhow!(message))),
        };
    }

    let is_pending = state.pending_device_flow.read().await.is_some();
    Ok(Json(if is_pending { DevicePollResponse::Pending } else { DevicePollResponse::NotStarted }))
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
            device_flow_result: RwLock::new(None),
            token_refresh_lock: tokio::sync::Mutex::new(()),
            cloudflare_tunnel: std::sync::Arc::new(crate::tunnel::CloudflareTunnel::new()),
            tailscale_tunnel: std::sync::Arc::new(crate::tailscale::TailscaleTunnel::new()),
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

    /// Rule-proving test: `device_poll` only ever reads state, it must never contact GitHub
    /// itself, proven by mounting no mock at all so any HTTP call would fail the request. Actually
    /// contacting GitHub is `run_device_flow_poller`'s job now, run server-side independent of
    /// whether a browser tab is polling, so an attempt still completes even if the tab that
    /// started it is closed the instant GitHub reports success.
    #[tokio::test]
    async fn device_poll_reports_pending_from_state_without_calling_github() {
        let mock_server = MockServer::start().await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Pending));
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

    /// Rule-proving test: the poller keeps the attempt alive across repeated
    /// "authorization_pending" responses (the operator hasn't answered yet, there's nothing to
    /// restart) instead of tearing it down after the first one, only ending once GitHub reports
    /// something terminal.
    #[tokio::test]
    async fn poller_keeps_the_attempt_alive_across_repeated_pending_polls_then_resolves() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "authorization_pending" })))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "access_denied" })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        run_device_flow_poller(state.clone(), "test-device-code".to_string(), 0).await;

        assert!(state.pending_device_flow.read().await.is_none(), "the attempt ends once GitHub reports a terminal outcome");
        assert!(token_queries::get(&state.db).await.unwrap().is_none());
        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Denied), "device_poll must surface the terminal outcome the poller recorded");
    }

    /// Rule-proving test: an explicit decline on GitHub's side must clear the attempt (so a stale
    /// poll can't resurrect it) and must not store any token.
    #[tokio::test]
    async fn poller_clears_the_attempt_and_records_denied() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "access_denied" })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        run_device_flow_poller(state.clone(), "test-device-code".to_string(), 0).await;

        assert!(state.pending_device_flow.read().await.is_none(), "a denied attempt must be cleared, not left for a stale poll to resurrect");
        assert!(token_queries::get(&state.db).await.unwrap().is_none());
        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Denied));
    }

    /// Rule-proving test: GitHub reporting the device code itself expired must clear the attempt
    /// and must not store any token.
    #[tokio::test]
    async fn poller_clears_the_attempt_and_records_expired_from_github() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "expired_token" })))
            .mount(&mock_server)
            .await;

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));

        run_device_flow_poller(state.clone(), "test-device-code".to_string(), 0).await;

        assert!(state.pending_device_flow.read().await.is_none());
        assert!(token_queries::get(&state.db).await.unwrap().is_none());
        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Expired));
    }

    /// Rule-proving test: once the locally-tracked deadline has passed, the poller must record
    /// `Expired` and clear the attempt *without* ever calling GitHub, proven here by mounting no
    /// mock at all, so any HTTP call would fail the request rather than silently succeed.
    #[tokio::test]
    async fn poller_reports_expired_locally_without_calling_github() {
        let mock_server = MockServer::start().await;
        // Deliberately no `Mock::given(...).mount(...)`, any request to this server 404s.

        let (state, user) = test_state(&mock_server).await;
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::seconds(-5)));

        run_device_flow_poller(state.clone(), "test-device-code".to_string(), 0).await;

        assert!(state.pending_device_flow.read().await.is_none());
        let Json(resp) = device_poll(State(state.clone()), CurrentUser(user)).await.unwrap();
        assert!(matches!(resp, DevicePollResponse::Expired));
    }

    // The success path (`DevicePollOutcome::Success` -> `persist_connection` -> `Connected`) isn't
    // covered by a poller-level test here: `persist_connection` validates the freshly-exchanged
    // token via `client::for_token`, which always targets the real github.com API (unlike the
    // mock-server-backed `state.github_client` other handlers' tests pre-seed) and so can't run
    // against a local mock in this test environment. The mapping itself
    // (`Ok(Connected { .. }) => DeviceFlowResult::Connected`) is straight-line code in
    // `run_device_flow_poller`; the denied/expired/superseded tests below cover the actual
    // regression this change fixes (the poller running independent of frontend polling at all).

    /// Rule-proving test: a `device_start` call that supersedes an in-flight attempt (a fresh
    /// "Connect GitHub" click before the previous code was used) must stop the old attempt's
    /// poller from clobbering the new one's state once GitHub eventually resolves the old code.
    #[tokio::test]
    async fn poller_for_a_superseded_attempt_does_not_touch_the_newer_attempts_state() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "access_denied" })))
            .mount(&mock_server)
            .await;

        let (state, _user) = test_state(&mock_server).await;
        // The old attempt is still recorded as "pending" from the poller's point of view, but a
        // newer attempt (a different device_code) has already replaced it in shared state, e.g.
        // because the operator clicked "Connect GitHub" a second time.
        *state.pending_device_flow.write().await = Some(pending(chrono::Duration::minutes(5)));
        *state.pending_device_flow.write().await = Some(oauth::PendingDeviceFlow {
            device_code: "newer-device-code".to_string(),
            interval_secs: 5,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        });

        run_device_flow_poller(state.clone(), "test-device-code".to_string(), 0).await;

        let still_pending = state.pending_device_flow.read().await.clone().unwrap();
        assert_eq!(still_pending.device_code, "newer-device-code", "the newer attempt must survive untouched");
        assert!(state.device_flow_result.read().await.is_none(), "the superseded attempt must not record a result for the newer one to see");
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
    /// must only be recoverable by decrypting through `state.enc`, not by, say, reading the
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
