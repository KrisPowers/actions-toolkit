use anyhow::Result;
use octocrab::Octocrab;

use crate::app::AppState;
use crate::db::models::{parse_iso, GithubToken};
use crate::db::queries::github_token as token_queries;
use crate::error::{AppError, AppResult};
use crate::github::oauth;

const NO_TOKEN_MESSAGE: &str = "no GitHub token has been configured; add one in Settings";

/// A `github_app` token is refreshed this far ahead of its stated expiry, so a call in flight
/// right at the boundary doesn't race GitHub rejecting it mid-request.
const REFRESH_THRESHOLD_MINUTES: i64 = 5;

/// Build (or fetch the cached) octocrab client authenticated with the account-wide GitHub
/// connection: a legacy PAT, or a GitHub App user-to-server token (refreshed here if it's expired
/// or close to it). Returns a client-facing 400 (not a 500) when no token has been configured yet,
/// since that's an expected, user-actionable state, not a server bug.
pub async fn shared(state: &AppState) -> AppResult<Octocrab> {
    if let Some(client) = state.github_client.read().await.clone() {
        return Ok(client);
    }

    let mut guard = state.github_client.write().await;
    if let Some(client) = guard.clone() {
        return Ok(client);
    }

    let row = token_queries::get(&state.db).await?.ok_or_else(|| AppError::BadRequest(NO_TOKEN_MESSAGE.into()))?;
    let token = if row.token_type == "github_app" {
        ensure_fresh_app_token(state, &row).await?
    } else {
        state.enc.decrypt_str(&row.token_encrypted, &row.token_nonce).map_err(AppError::Internal)?
    };

    let client = Octocrab::builder().personal_token(token).build().map_err(|e| AppError::Internal(e.into()))?;
    *guard = Some(client.clone());
    Ok(client)
}

/// Returns a current access token for a `github_app` row, refreshing it first if it's expired or
/// within `REFRESH_THRESHOLD_MINUTES` of expiring. A failed refresh (revoked refresh token,
/// network error) marks the connection as needing reconnect rather than handing back a stale
/// token that would just fail the next GitHub call with a confusing 401.
async fn ensure_fresh_app_token(state: &AppState, row: &GithubToken) -> AppResult<String> {
    let needs_refresh = match row.expires_at.as_deref() {
        Some(expires_at) => parse_iso(expires_at) <= chrono::Utc::now() + chrono::Duration::minutes(REFRESH_THRESHOLD_MINUTES),
        None => true,
    };

    if !needs_refresh {
        return state.enc.decrypt_str(&row.token_encrypted, &row.token_nonce).map_err(AppError::Internal);
    }

    let (refresh_ct, refresh_nonce) = row
        .refresh_token_encrypted
        .as_deref()
        .zip(row.refresh_token_nonce.as_deref())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("github_app token row is missing its refresh token")))?;
    let refresh_token = state.enc.decrypt_str(refresh_ct, refresh_nonce).map_err(AppError::Internal)?;

    match oauth::refresh_access_token(&state.config.github_oauth_token_url, &state.config.github_app_client_id, &refresh_token).await {
        Ok(refreshed) => {
            let (token_encrypted, token_nonce) = state.enc.encrypt_str(&refreshed.access_token).map_err(AppError::Internal)?;
            let (refresh_encrypted, new_refresh_nonce) = state.enc.encrypt_str(&refreshed.refresh_token).map_err(AppError::Internal)?;
            let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(refreshed.expires_in)).to_rfc3339();
            token_queries::update_after_refresh(&state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &new_refresh_nonce, &expires_at)
                .await?;
            Ok(refreshed.access_token)
        }
        Err(e) => {
            tracing::warn!(error = %e, "GitHub App token refresh failed; marking the connection as needing reconnect");
            token_queries::mark_needs_reconnect(&state.db).await?;
            Err(AppError::NeedsReconnect("the stored GitHub App refresh token was rejected".to_string()))
        }
    }
}

/// Clears the cached client so the next call to `shared` re-reads the token from the database.
/// Call this after rotating, removing, or refreshing the token.
pub async fn invalidate(state: &AppState) {
    *state.github_client.write().await = None;
}

/// Build a one-off client for a token that hasn't been saved yet, used to validate a token
/// (and discover the authenticated login) before it's persisted.
pub fn for_token(token: &str) -> Result<Octocrab> {
    Ok(Octocrab::builder().personal_token(token.to_string()).build()?)
}

/// Decrypt and return the raw configured token string, e.g. for git checkout auth where an
/// octocrab client isn't usable directly.
pub async fn decrypted_token(state: &AppState) -> Result<String> {
    let row = token_queries::get(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!(NO_TOKEN_MESSAGE))?;
    state.enc.decrypt_str(&row.token_encrypted, &row.token_nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, AppStateInner};
    use crate::auth::jwt::JwtCodec;
    use crate::config::AppConfig;
    use crate::crypto::EncryptionKey;
    use crate::runner::log_stream::LogHub;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_state(token_url: String) -> AppState {
        let test_id = uuid::Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-client-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let db = crate::db::connect(&data_dir.join("db.sqlite")).await.unwrap();
        let enc = EncryptionKey::load_or_generate(None, &data_dir.join("secrets")).unwrap();
        let config = AppConfig { data_dir, github_app_client_id: "test-client-id".to_string(), github_oauth_token_url: token_url };

        AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt: JwtCodec::new("test-secret"),
            enc,
            docker: None,
            bucket_capability_ok: true,
            log_hub: Arc::new(LogHub::new()),
            github_client: RwLock::new(None),
            oauth_states: Default::default(),
        }))
    }

    /// Rule-proving test: when GitHub rejects a refresh (revoked/expired refresh token), the
    /// connection is marked `needs_reconnect` and `shared` returns `NeedsReconnect` instead of
    /// building a client from a stale token and letting the next GitHub call fail with a
    /// confusing 401 further downstream.
    #[tokio::test]
    async fn failed_refresh_marks_needs_reconnect_instead_of_returning_a_stale_client() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "error": "bad_refresh_token",
                "error_description": "The refresh token passed is incorrect or expired."
            })))
            .mount(&mock_server)
            .await;

        let state = test_state(mock_server.uri()).await;

        let (token_encrypted, token_nonce) = state.enc.encrypt_str("stale-access-token").unwrap();
        let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str("revoked-refresh-token").unwrap();
        let expired_at = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        token_queries::upsert_app_token(&state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &expired_at, None, "octocat")
            .await
            .unwrap();

        let result = shared(&state).await;
        assert!(matches!(result, Err(AppError::NeedsReconnect(_))));

        let row = token_queries::get(&state.db).await.unwrap().unwrap();
        assert_eq!(row.needs_reconnect, 1);
    }

    #[tokio::test]
    async fn fresh_app_token_is_used_without_refreshing() {
        // No mock registered at all: if this reached out to GitHub, the request would fail to
        // connect and the test would error, proving a non-expiring token skips the refresh call.
        let mock_server = MockServer::start().await;
        let state = test_state(mock_server.uri()).await;

        let (token_encrypted, token_nonce) = state.enc.encrypt_str("still-fresh-access-token").unwrap();
        let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str("refresh-token").unwrap();
        let far_future = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        token_queries::upsert_app_token(&state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &far_future, None, "octocat")
            .await
            .unwrap();

        assert!(shared(&state).await.is_ok());
    }
}
