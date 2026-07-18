use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::GithubTokenStatus;
use crate::db::queries::github_token as token_queries;
use crate::error::{AppError, AppResult};
use crate::github::{client, discovery};

pub async fn status(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<GithubTokenStatus>> {
    let row = token_queries::get(&state.db).await?;
    Ok(Json(match row {
        Some(t) => GithubTokenStatus {
            connected: true,
            github_login: Some(t.github_login),
            scopes: Some(t.scopes),
            connected_at: Some(t.updated_at),
            token_type: Some(t.token_type.clone()),
            // The full "a `pat` row always needs reconnect" policy lands in the migration issue
            // that follows this one; for now this reflects the stored flag as-is.
            needs_reconnect: t.needs_reconnect != 0,
        },
        None => GithubTokenStatus {
            connected: false,
            github_login: None,
            scopes: None,
            connected_at: None,
            token_type: None,
            needs_reconnect: false,
        },
    }))
}

#[derive(Deserialize)]
pub struct SetTokenRequest {
    pub token: String,
}

pub async fn set_token(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(req): Json<SetTokenRequest>,
) -> AppResult<Json<GithubTokenStatus>> {
    let token = req.token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest("token is required".into()));
    }

    let client = client::for_token(token).map_err(AppError::Internal)?;
    let login = discovery::validate_token(&client)
        .await
        .map_err(|e| AppError::BadRequest(format!("GitHub rejected this token: {e}")))?;

    let (token_encrypted, token_nonce) = state.enc.encrypt_str(token).map_err(AppError::Internal)?;
    token_queries::upsert(&state.db, &token_encrypted, &token_nonce, &login, "").await?;
    client::invalidate(&state).await;

    Ok(Json(GithubTokenStatus {
        connected: true,
        github_login: Some(login),
        scopes: Some(String::new()),
        connected_at: None,
        token_type: Some("pat".to_string()),
        needs_reconnect: false,
    }))
}

pub async fn delete_token(State(state): State<AppState>, _user: CurrentUser) -> AppResult<()> {
    token_queries::delete(&state.db).await?;
    client::invalidate(&state).await;
    Ok(())
}

pub async fn accessible_repos(
    State(state): State<AppState>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<discovery::AccessibleRepo>>> {
    let client = client::shared(&state).await?;

    // A `github_app` connection with an installation ID lists exactly what that installation
    // was granted; everything else (a legacy PAT, or an App connection without one, e.g. an
    // older install) falls back to the account-wide listing the token can see.
    let row = token_queries::get(&state.db).await?;
    let repos = match row.as_ref().and_then(|t| (t.token_type == "github_app").then_some(t.installation_id).flatten()) {
        Some(installation_id) => discovery::list_accessible_repos_for_installation(&client, installation_id)
            .await
            .map_err(AppError::Internal)?,
        None => discovery::list_accessible_repos(&client).await.map_err(AppError::Internal)?,
    };
    Ok(Json(repos))
}
