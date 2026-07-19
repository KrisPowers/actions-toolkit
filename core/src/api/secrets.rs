use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::Secret;
use crate::db::queries::secrets as secret_queries;
use crate::error::{AppError, AppResult};

fn validate_name(name: &str) -> AppResult<()> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !name.chars().next().unwrap().is_ascii_digit();
    if !valid {
        return Err(AppError::BadRequest(
            "secret name must be UPPER_SNAKE_CASE (letters, digits, underscore; can't start with a digit), matching env var naming".into(),
        ));
    }
    Ok(())
}

pub async fn list_for_repo(State(state): State<AppState>, Path(repo_id): Path<String>, _user: CurrentUser) -> AppResult<Json<Vec<Secret>>> {
    Ok(Json(secret_queries::list_for_repo(&state.db, &repo_id).await?))
}

#[derive(Deserialize)]
pub struct CreateSecretRequest {
    pub name: String,
    pub value: String,
}

pub async fn create(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateSecretRequest>,
) -> AppResult<Json<Secret>> {
    validate_name(&req.name)?;
    if req.value.is_empty() {
        return Err(AppError::BadRequest("value is required".into()));
    }

    let (value_encrypted, value_nonce) = state.enc.encrypt_str(&req.value).map_err(AppError::Internal)?;
    let secret = secret_queries::upsert(&state.db, &repo_id, &req.name, &value_encrypted, &value_nonce, &user.id).await?;
    Ok(Json(secret))
}

pub async fn delete(State(state): State<AppState>, Path((_repo_id, id)): Path<(String, String)>, _user: CurrentUser) -> AppResult<()> {
    secret_queries::find_by_id(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    secret_queries::delete(&state.db, &id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_accepts_upper_snake_case() {
        assert!(validate_name("NPM_TOKEN").is_ok());
        assert!(validate_name("A").is_ok());
        assert!(validate_name("A1_B2").is_ok());
    }

    #[test]
    fn validate_name_rejects_anything_not_upper_snake_case() {
        assert!(validate_name("").is_err());
        assert!(validate_name("npm_token").is_err(), "lowercase should be rejected to match env var convention");
        assert!(validate_name("1STARTS_WITH_DIGIT").is_err());
        assert!(validate_name("HAS-DASH").is_err());
        assert!(validate_name("HAS SPACE").is_err());
        assert!(validate_name("GITHUB_TOKEN").is_ok(), "collision with the auto-injected var is allowed here; the runner decides precedence");
    }

    use crate::app::{AppState, AppStateInner};
    use crate::auth::jwt::JwtCodec;
    use crate::config::AppConfig;
    use crate::crypto::EncryptionKey;
    use crate::db::models::{Repo, User};
    use crate::db::queries::{repos as repo_queries, users as user_queries};
    use crate::runner::log_stream::LogHub;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    async fn test_state() -> (AppState, User, Repo) {
        let test_id = uuid::Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-secrets-test-{test_id}"));
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
        let repo = repo_queries::create(&db, "octocat", "hello-world", "main", b"secret", b"nonce", &user.id).await.unwrap();

        let state = AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt: JwtCodec::new("test-secret"),
            enc,
            docker: None,
            bucket_capability_ok: true,
            log_hub: Arc::new(LogHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
        }));

        (state, user, repo)
    }

    /// Rule-proving test: creating a secret must never return its plaintext value in the API
    /// response, and the value must be recoverable only by decrypting through `state.enc`.
    #[tokio::test]
    async fn create_never_returns_the_plaintext_value() {
        let (state, user, repo) = test_state().await;

        let Json(secret) = create(
            State(state.clone()),
            Path(repo.id.clone()),
            CurrentUser(user),
            Json(CreateSecretRequest { name: "NPM_TOKEN".to_string(), value: "npm_supersecretvalue123".to_string() }),
        )
        .await
        .unwrap();

        let json = serde_json::to_value(&secret).unwrap();
        let json_text = json.to_string();
        assert!(!json_text.contains("npm_supersecretvalue123"), "the plaintext value must never appear in the API response: {json_text}");

        let stored = secret_queries::find_by_id(&state.db, &secret.id).await.unwrap().unwrap();
        let decrypted = state.enc.decrypt_str(&stored.value_encrypted, &stored.value_nonce).unwrap();
        assert_eq!(decrypted, "npm_supersecretvalue123");
    }

    /// Rule-proving test: listing secrets for a repo must never include any value field at all,
    /// only metadata.
    #[tokio::test]
    async fn list_never_returns_values() {
        let (state, user, repo) = test_state().await;
        let _ = create(
            State(state.clone()),
            Path(repo.id.clone()),
            CurrentUser(user.clone()),
            Json(CreateSecretRequest { name: "API_KEY".to_string(), value: "sk-supersecretvalue456".to_string() }),
        )
        .await
        .unwrap();

        let Json(list) = list_for_repo(State(state.clone()), Path(repo.id.clone()), CurrentUser(user)).await.unwrap();

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "API_KEY");
        let json_text = serde_json::to_string(&list).unwrap();
        assert!(!json_text.contains("sk-supersecretvalue456"));
        assert!(!json_text.contains("value"), "no value-shaped field should be serialized at all: {json_text}");
    }

    /// Rule-proving test: creating a secret with the same name as an existing one for the same
    /// repo rotates its value in place rather than erroring on the unique constraint.
    #[tokio::test]
    async fn creating_a_secret_with_an_existing_name_rotates_its_value() {
        let (state, user, repo) = test_state().await;
        let _ = create(
            State(state.clone()),
            Path(repo.id.clone()),
            CurrentUser(user.clone()),
            Json(CreateSecretRequest { name: "TOKEN".to_string(), value: "old-value".to_string() }),
        )
        .await
        .unwrap();
        let _ = create(
            State(state.clone()),
            Path(repo.id.clone()),
            CurrentUser(user),
            Json(CreateSecretRequest { name: "TOKEN".to_string(), value: "new-value".to_string() }),
        )
        .await
        .unwrap();

        let all = secret_queries::list_for_repo(&state.db, &repo.id).await.unwrap();
        assert_eq!(all.len(), 1, "rotating a secret must not leave a duplicate row behind");
        let decrypted = state.enc.decrypt_str(&all[0].value_encrypted, &all[0].value_nonce).unwrap();
        assert_eq!(decrypted, "new-value");
    }

    #[tokio::test]
    async fn create_rejects_an_invalid_name() {
        let (state, user, repo) = test_state().await;
        let result = create(
            State(state),
            Path(repo.id),
            CurrentUser(user),
            Json(CreateSecretRequest { name: "not-valid".to_string(), value: "x".to_string() }),
        )
        .await;
        assert!(result.is_err());
    }
}
