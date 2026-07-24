use axum::extract::State;
use axum::Json;

use crate::app::AppState;
use crate::auth::middleware::ApprovedUser;
use crate::db::models::{GithubToken, GithubTokenStatus};
use crate::db::queries::{github_installations as installations_queries, github_token as token_queries};
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

pub async fn status(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<Json<GithubTokenStatus>> {
    let row = token_queries::get(&state.db).await?;
    Ok(Json(to_status(row)))
}

pub async fn delete_token(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<()> {
    token_queries::delete(&state.db).await?;
    client::invalidate(&state).await;
    Ok(())
}

/// Lists repos across every installation this instance's account-wide connection currently has
/// (personal account and any org), so repos from more than one org are all visible to the
/// connect-repo picker in one go. Falls back to the legacy account-wide listing when there are no
/// stored installations (a `pat` connection, or an App connection made before installation
/// discovery existed).
pub async fn accessible_repos(
    State(state): State<AppState>,
    _user: ApprovedUser,
) -> AppResult<Json<Vec<discovery::AccessibleRepo>>> {
    let client = client::shared(&state).await?;

    let installations = installations_queries::list(&state.db).await?;
    let repos = if installations.is_empty() {
        discovery::list_accessible_repos(&client).await.map_err(AppError::Internal)?
    } else {
        let mut merged = Vec::new();
        for install in installations {
            let mut repos = discovery::list_accessible_repos_for_installation(&client, install.id).await.map_err(AppError::Internal)?;
            for repo in &mut repos {
                repo.account_login = Some(install.account_login.clone());
                repo.account_type = Some(install.account_type.clone());
            }
            merged.extend(repos);
        }
        merged
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
