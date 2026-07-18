use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::queries::github_token as token_queries;
use crate::github::{client, discovery, oauth};

/// Builds this instance's OAuth callback URL from the request's own `Host` header (and
/// `X-Forwarded-Proto` if this is behind a tunnel/proxy terminating TLS) rather than a fixed
/// config value, since actions-toolkit runs on whatever host:port the operator chose. That value
/// must match one of the callback URLs registered on the GitHub App, or GitHub rejects the
/// authorize/exchange request outright.
fn callback_redirect_uri(headers: &HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:7890");
    let scheme = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).unwrap_or("http");
    format!("{scheme}://{host}/api/auth/github/callback")
}

/// Starts a connect attempt: generates a fresh PKCE verifier/challenge and CSRF state, stashes
/// the verifier server-side keyed by the state, and sends the browser to GitHub's authorize
/// screen. Gated behind `CurrentUser` (a full browser navigation still carries the session
/// cookie, `SameSite=Lax` allows that) so only a logged-in operator can kick off a connect that
/// would replace the instance-wide GitHub connection.
pub async fn authorize(State(state): State<AppState>, CurrentUser(_user): CurrentUser, headers: HeaderMap) -> Redirect {
    // Opportunistic sweep of abandoned attempts (started but never completed) so this map stays
    // bounded on a long-running instance without needing a background task.
    state.oauth_states.retain(|_, pending| !pending.is_expired());

    let pkce = oauth::generate_pkce();
    let csrf_state = oauth::generate_state();
    state
        .oauth_states
        .insert(csrf_state.clone(), oauth::PendingAuthorize { code_verifier: pkce.verifier, created_at: chrono::Utc::now() });

    let redirect_uri = callback_redirect_uri(&headers);
    let url = oauth::authorize_url(&state.config.github_app_client_id, &redirect_uri, &csrf_state, &pkce.challenge);
    Redirect::to(&url)
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    installation_id: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
}

enum CallbackOutcome {
    Denied,
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for CallbackOutcome {
    fn from(e: anyhow::Error) -> Self {
        CallbackOutcome::Failed(e)
    }
}

/// Completes a connect attempt. Always redirects back into the UI rather than returning a raw
/// error status, since this response is a full browser navigation, not a fetch a frontend script
/// can inspect. State validation happens before any GitHub call, so a bad/replayed/expired state
/// never triggers a token exchange.
pub async fn callback(
    State(state): State<AppState>,
    CurrentUser(_user): CurrentUser,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Redirect {
    match complete(&state, &headers, q).await {
        Ok(()) => Redirect::to("/settings?github=connected"),
        Err(CallbackOutcome::Denied) => Redirect::to("/settings?github=denied"),
        Err(CallbackOutcome::Failed(e)) => {
            tracing::error!(error = %e, "GitHub OAuth callback failed");
            Redirect::to("/settings?github=error")
        }
    }
}

async fn complete(state: &AppState, headers: &HeaderMap, q: CallbackQuery) -> Result<(), CallbackOutcome> {
    if let Some(err) = q.error {
        tracing::warn!(error = %err, description = ?q.error_description, "GitHub OAuth authorization was not granted");
        return Err(CallbackOutcome::Denied);
    }

    let pending = oauth::take_pending(&state.oauth_states, q.state.as_deref())?;
    let code = q.code.ok_or_else(|| anyhow::anyhow!("callback had no code parameter"))?;

    let redirect_uri = callback_redirect_uri(headers);
    let exchanged = oauth::exchange_code(&state.config.github_app_client_id, &code, &pending.code_verifier, &redirect_uri).await?;

    let github_client = client::for_token(&exchanged.access_token)?;
    let login = discovery::validate_token(&github_client).await?;

    let (token_encrypted, token_nonce) = state.enc.encrypt_str(&exchanged.access_token)?;
    let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str(&exchanged.refresh_token)?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(exchanged.expires_in)).to_rfc3339();

    token_queries::upsert_app_token(
        &state.db,
        &token_encrypted,
        &token_nonce,
        &refresh_encrypted,
        &refresh_nonce,
        &expires_at,
        q.installation_id,
        &login,
    )
    .await
    .map_err(anyhow::Error::from)?;

    client::invalidate(state).await;
    Ok(())
}
