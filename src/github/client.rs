use anyhow::Result;
use octocrab::Octocrab;

use crate::app::AppState;
use crate::db::queries::github_token as token_queries;
use crate::error::{AppError, AppResult};

const NO_TOKEN_MESSAGE: &str = "no GitHub token has been configured; add one in Settings";

/// Build (or fetch the cached) octocrab client authenticated with the single account-wide
/// GitHub token entered during setup. Returns a client-facing 400 (not a 500) when no token
/// has been configured yet, since that's an expected, user-actionable state, not a server bug.
pub async fn shared(state: &AppState) -> AppResult<Octocrab> {
    if let Some(client) = state.github_client.read().await.clone() {
        return Ok(client);
    }

    let mut guard = state.github_client.write().await;
    if let Some(client) = guard.clone() {
        return Ok(client);
    }

    let row = token_queries::get(&state.db).await?.ok_or_else(|| AppError::BadRequest(NO_TOKEN_MESSAGE.into()))?;
    let token = state.enc.decrypt_str(&row.token_encrypted, &row.token_nonce).map_err(AppError::Internal)?;
    let client = Octocrab::builder().personal_token(token).build().map_err(|e| AppError::Internal(e.into()))?;
    *guard = Some(client.clone());
    Ok(client)
}

/// Clears the cached client so the next call to `shared` re-reads the token from the database.
/// Call this after rotating or removing the token.
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
