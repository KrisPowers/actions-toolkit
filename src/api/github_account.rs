use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::{GithubToken, GithubTokenStatus};
use crate::db::queries::github_token as token_queries;
use crate::error::{AppError, AppResult};
use crate::github::{client, discovery};

/// Maps a stored token row to the client-facing status. Pure and separate from the `status`
/// handler so the "does an existing install correctly get prompted to reconnect" policy is
/// directly testable against a hand-built fixture, without needing a running server or session.
fn to_status(row: Option<GithubToken>) -> GithubTokenStatus {
    match row {
        Some(t) => GithubTokenStatus {
            connected: true,
            github_login: Some(t.github_login),
            scopes: Some(t.scopes),
            connected_at: Some(t.updated_at),
            token_type: Some(t.token_type.clone()),
            // A legacy `pat` row always prompts reconnect, since the point is migrating every
            // existing install off it; a `github_app` row only prompts when a refresh actually
            // failed (`client::shared` sets this flag, see the token-refresh issue).
            needs_reconnect: t.token_type == "pat" || t.needs_reconnect != 0,
        },
        None => GithubTokenStatus {
            connected: false,
            github_login: None,
            scopes: None,
            connected_at: None,
            token_type: None,
            needs_reconnect: false,
        },
    }
}

pub async fn status(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<GithubTokenStatus>> {
    let row = token_queries::get(&state.db).await?;
    Ok(Json(to_status(row)))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule-proving test for the milestone's "nothing breaks on upgrade" rule: a database that
    /// has only ever gone through the legacy PAT flow (a row inserted using just the original
    /// pre-migration columns, letting every App-only column take its schema default rather than
    /// anything application code sets) still boots, and its status correctly prompts reconnect.
    #[tokio::test]
    async fn legacy_pat_only_database_reports_needs_reconnect_after_migrating() {
        let test_id = uuid::Uuid::new_v4().to_string();
        let db_path = std::env::temp_dir().join(format!("atk-legacy-pat-fixture-{test_id}.db"));
        let pool = crate::db::connect(&db_path).await.expect("a database with only a pre-App-migration PAT row should still migrate cleanly");

        sqlx::query(
            "INSERT INTO github_token (id, token_encrypted, token_nonce, github_login, scopes, created_at, updated_at) \
             VALUES (1, ?, ?, 'octocat', 'repo', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
        )
        .bind(b"legacy-ciphertext".as_slice())
        .bind(b"legacy-nonce".as_slice())
        .execute(&pool)
        .await
        .unwrap();

        let row = token_queries::get(&pool).await.unwrap();
        assert_eq!(row.as_ref().unwrap().token_type, "pat", "the new column should default to 'pat' with no application code involved");
        assert_eq!(row.as_ref().unwrap().token_encrypted, b"legacy-ciphertext", "the pre-existing PAT must still be readable, untouched");

        let status = to_status(row);
        assert!(status.connected);
        assert!(status.needs_reconnect, "an existing PAT install must be prompted to reconnect");
        assert_eq!(status.token_type.as_deref(), Some("pat"));

        let _ = std::fs::remove_file(&db_path);
    }
}
