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
///
/// A cache hit is only trusted once the underlying `github_app` row (if any) has been checked for
/// expiry: the client used to be cached unconditionally after the first build, which meant a
/// `github_app` token's 8-hour expiry was never re-checked for the rest of the process's life and
/// every call through here silently 401'd against GitHub once it passed. Tests that pre-seed
/// `github_client` without any DB row (to exercise unrelated logic without a real connection)
/// still hit the fast path, since `cached_app_token_needs_refresh` only has an opinion when a
/// `github_app` row actually exists.
pub async fn shared(state: &AppState) -> AppResult<Octocrab> {
    if let Some(client) = state.github_client.read().await.clone() {
        if !cached_app_token_needs_refresh(state).await? {
            return Ok(client);
        }
    }

    let mut guard = state.github_client.write().await;
    if let Some(client) = guard.clone() {
        if !cached_app_token_needs_refresh(state).await? {
            return Ok(client);
        }
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

async fn cached_app_token_needs_refresh(state: &AppState) -> AppResult<bool> {
    match token_queries::get(&state.db).await? {
        Some(row) if row.token_type == "github_app" => Ok(app_token_needs_refresh(&row)),
        _ => Ok(false),
    }
}

fn app_token_needs_refresh(row: &GithubToken) -> bool {
    match row.expires_at.as_deref() {
        Some(expires_at) => parse_iso(expires_at) <= chrono::Utc::now() + chrono::Duration::minutes(REFRESH_THRESHOLD_MINUTES),
        None => true,
    }
}

/// Returns a current access token for a `github_app` row, refreshing it first if it's expired or
/// within `REFRESH_THRESHOLD_MINUTES` of expiring. A failed refresh (revoked refresh token,
/// network error) marks the connection as needing reconnect rather than handing back a stale
/// token that would just fail the next GitHub call with a confusing 401.
async fn ensure_fresh_app_token(state: &AppState, row: &GithubToken) -> AppResult<String> {
    if !app_token_needs_refresh(row) {
        return state.enc.decrypt_str(&row.token_encrypted, &row.token_nonce).map_err(AppError::Internal);
    }

    // GitHub App refresh tokens are single-use: a concurrent caller (e.g. two workflow runs
    // checking out code around the same moment) could otherwise read the same soon-to-expire row
    // and race this exchange, with the loser's refresh token already spent by the winner. Without
    // this lock the loser gets rejected and wrongly marks a connection the winner just refreshed
    // fine as needing reconnect.
    let _refresh_guard = state.token_refresh_lock.lock().await;

    // Re-read after acquiring the lock: a caller that was waiting on it may have lost the race to
    // one that already refreshed, in which case the row is fresh now and there's nothing to do.
    let row = token_queries::get(&state.db)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("github_app token row disappeared during refresh")))?;
    if !app_token_needs_refresh(&row) {
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
/// octocrab client isn't usable directly. Goes through the same refresh check as `shared` for a
/// `github_app` row, so checkout doesn't hand git a token that's about to expire mid-clone.
pub async fn decrypted_token(state: &AppState) -> Result<String> {
    let row = token_queries::get(&state.db)
        .await?
        .ok_or_else(|| anyhow::anyhow!(NO_TOKEN_MESSAGE))?;

    if row.token_type == "github_app" {
        return Ok(ensure_fresh_app_token(state, &row).await?);
    }

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
        let config = AppConfig {
            data_dir,
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: token_url,
            github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
        };

        AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt: JwtCodec::new("test-secret"),
            enc,
            docker: None,
            bucket_capability_ok: true,
            bucket_capability_reason: None,
            log_hub: Arc::new(LogHub::new()),
            stats_hub: Arc::new(crate::runner::stats_hub::StatsHub::new()),
            activity_hub: Arc::new(crate::runner::activity_hub::ActivityHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
            device_flow_result: RwLock::new(None),
            login_flows: RwLock::new(std::collections::HashMap::new()),
            login_rate_limiter: atk_auth::rate_limit::RateLimiter::new(
                crate::auth::login_flow::LOGIN_RATE_LIMIT_MAX_ATTEMPTS,
                crate::auth::login_flow::LOGIN_RATE_LIMIT_WINDOW,
            ),
            token_refresh_lock: tokio::sync::Mutex::new(()),
            cloudflare_tunnel: std::sync::Arc::new(crate::tunnel::CloudflareTunnel::new()),
            tailscale_tunnel: std::sync::Arc::new(crate::tailscale::TailscaleTunnel::new()),
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

    /// Rule-proving test: GitHub App refresh tokens are single-use, so two callers racing to
    /// refresh the same about-to-expire token (e.g. two workflow runs checking out code around the
    /// same moment) must not both spend it. Only the winner should actually call GitHub; the
    /// loser, once unblocked by `token_refresh_lock`, must see the already-refreshed row and reuse
    /// it rather than racing a second (rejected) exchange and wrongly flipping `needs_reconnect`.
    #[tokio::test]
    async fn concurrent_refreshes_do_not_race_the_single_use_refresh_token() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "refreshed-access-token",
                "refresh_token": "refreshed-refresh-token",
                "expires_in": 28800
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;
        // Any second exchange (the bug this test guards against) hits this instead and would be
        // rejected, since the refresh token the first exchange used is now spent.
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "error": "bad_refresh_token",
                "error_description": "The refresh token passed is incorrect or expired."
            })))
            .mount(&mock_server)
            .await;

        let state = test_state(mock_server.uri()).await;
        let (token_encrypted, token_nonce) = state.enc.encrypt_str("stale-access-token").unwrap();
        let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str("still-valid-refresh-token").unwrap();
        let already_expired = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        token_queries::upsert_app_token(
            &state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &already_expired, None, "octocat",
        )
        .await
        .unwrap();

        let (a, b) = tokio::join!(decrypted_token(&state), decrypted_token(&state));
        assert!(a.is_ok(), "first concurrent refresh should succeed: {a:?}");
        assert!(b.is_ok(), "second concurrent refresh should reuse the winner's result instead of racing GitHub: {b:?}");

        let row = token_queries::get(&state.db).await.unwrap().unwrap();
        assert_eq!(row.needs_reconnect, 0, "a concurrent refresh race must not wrongly mark a working connection as needing reconnect");
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

    /// `decrypted_token` (used for git checkout auth) must not hand out a token that's about to
    /// expire mid-clone: for a `github_app` row it goes through the same refresh check as
    /// `shared`, and the caller shouldn't need to know or care which token type is active.
    #[tokio::test]
    async fn decrypted_token_refreshes_an_expiring_app_token_transparently() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "refreshed-access-token",
                "refresh_token": "refreshed-refresh-token",
                "expires_in": 28800
            })))
            .mount(&mock_server)
            .await;

        let state = test_state(mock_server.uri()).await;
        let (token_encrypted, token_nonce) = state.enc.encrypt_str("stale-access-token").unwrap();
        let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str("still-valid-refresh-token").unwrap();
        let already_expired = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        token_queries::upsert_app_token(
            &state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &already_expired, None, "octocat",
        )
        .await
        .unwrap();

        let token = decrypted_token(&state).await.unwrap();
        assert_eq!(token, "refreshed-access-token");
    }

    #[tokio::test]
    async fn decrypted_token_passes_a_pat_row_through_without_touching_the_refresh_endpoint() {
        // No mock registered: a PAT row must never call the token endpoint at all.
        let mock_server = MockServer::start().await;
        let state = test_state(mock_server.uri()).await;
        let (token_encrypted, token_nonce) = state.enc.encrypt_str("my-legacy-pat").unwrap();
        token_queries::upsert(&state.db, &token_encrypted, &token_nonce, "octocat", "repo").await.unwrap();

        let token = decrypted_token(&state).await.unwrap();
        assert_eq!(token, "my-legacy-pat");
    }

    #[derive(Clone)]
    struct BufWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for BufWriter {
        type Writer = BufWriter;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// Rule-proving test for milestone #1's storage rule: a token refresh (success case, since
    /// that's the path that actually handles a live access/refresh token pair) never writes
    /// either token to a log line at any level, checked by capturing every tracing event emitted
    /// during the call and asserting the raw values never appear in it, not just checking for
    /// specific field names a future log line could easily bypass.
    #[tokio::test]
    async fn refreshing_a_token_never_logs_the_access_or_refresh_token() {
        let plaintext_old_access = "stale-access-token-should-never-be-logged";
        let plaintext_new_access = "refreshed-access-token-should-never-be-logged";
        let plaintext_new_refresh = "refreshed-refresh-token-should-never-be-logged";
        let plaintext_old_refresh = "old-refresh-token-should-never-be-logged";

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": plaintext_new_access,
                "refresh_token": plaintext_new_refresh,
                "expires_in": 28800
            })))
            .mount(&mock_server)
            .await;

        let state = test_state(mock_server.uri()).await;
        let (token_encrypted, token_nonce) = state.enc.encrypt_str(plaintext_old_access).unwrap();
        let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str(plaintext_old_refresh).unwrap();
        let already_expired = (chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339();
        token_queries::upsert_app_token(
            &state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &already_expired, None, "octocat",
        )
        .await
        .unwrap();

        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(BufWriter(buf.clone()))
            .with_max_level(tracing::Level::TRACE)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        shared(&state).await.unwrap();

        drop(_guard);
        let captured = String::from_utf8_lossy(&buf.lock().unwrap()).to_string();
        assert!(!captured.contains(plaintext_old_access));
        assert!(!captured.contains(plaintext_new_access));
        assert!(!captured.contains(plaintext_new_refresh));
        assert!(!captured.contains(plaintext_old_refresh));
    }
}
