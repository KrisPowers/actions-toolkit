pub use atk_config::*;

use std::path::PathBuf;

use anyhow::Result;
use sqlx::SqlitePool;

use crate::crypto::EncryptionKey;

pub struct Bootstrapped {
    pub db: SqlitePool,
    pub app_config: AppConfig,
    pub enc: EncryptionKey,
    pub jwt_secret: String,
}

/// Create the data/workspaces/artifacts directories, open (and migrate) the database, and
/// load-or-generate the JWT signing secret and encryption key. Shared by `init` and `start` so
/// both produce an identical, ready-to-serve data directory.
pub async fn bootstrap(
    data_dir: PathBuf,
    jwt_secret: Option<String>,
    encryption_key: Option<String>,
    github_app_client_id: Option<String>,
) -> Result<Bootstrapped> {
    let app_config = AppConfig {
        data_dir,
        github_app_client_id: github_app_client_id.unwrap_or_else(|| DEFAULT_GITHUB_APP_CLIENT_ID.to_string()),
        github_oauth_token_url: crate::github::oauth::GITHUB_TOKEN_URL.to_string(),
        github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
    };
    std::fs::create_dir_all(&app_config.data_dir)?;
    std::fs::create_dir_all(app_config.workspaces_dir())?;
    std::fs::create_dir_all(app_config.artifacts_dir())?;
    std::fs::create_dir_all(app_config.buckets_dir())?;

    let db = crate::db::connect(&app_config.db_path()).await?;
    tracing::info!(path = %app_config.db_path().display(), "database ready");

    seed_detected_host_paths(&db).await?;

    let enc = EncryptionKey::load_or_generate(encryption_key.as_deref(), &app_config.secrets_dir())?;

    let jwt_secret = match jwt_secret {
        Some(s) => s,
        None => {
            let path = app_config.secrets_dir().join("jwt.key");
            std::fs::create_dir_all(&app_config.secrets_dir())?;
            if path.exists() {
                std::fs::read_to_string(&path)?.trim().to_string()
            } else {
                use rand::RngCore;
                let mut bytes = [0u8; 32];
                rand::rngs::OsRng.fill_bytes(&mut bytes);
                let secret = hex::encode(bytes);
                std::fs::write(&path, &secret)?;
                secret
            }
        }
    };

    Ok(Bootstrapped { db, app_config, enc, jwt_secret })
}

/// So a fresh install can build a Rust/Node/Python/Go project without the operator ever opening
/// Settings first: on the very first bootstrap (before anyone has saved settings at all, detected
/// by `created_at == updated_at`, the migration-seeded row's initial state), scan the host for the
/// same common toolchain directories `suggested_host_paths` would offer and pre-populate the
/// "Extra host paths" allowlist with them. Skipped once the row has ever been updated (including
/// by the operator deliberately clearing it back to empty) so this never fights a real decision.
async fn seed_detected_host_paths(db: &sqlx::SqlitePool) -> Result<()> {
    seed_detected_host_paths_at(db, dirs::home_dir().as_deref()).await
}

async fn seed_detected_host_paths_at(db: &sqlx::SqlitePool, home: Option<&std::path::Path>) -> Result<()> {
    let settings = crate::db::queries::settings::get(db).await?;
    if settings.created_at != settings.updated_at || settings.bucket_host_mounts_json != "[]" {
        return Ok(());
    }
    let Some(home) = home else { return Ok(()) };
    let suggestions = crate::runner::host_toolchains::detect(home, &[]);
    if suggestions.is_empty() {
        return Ok(());
    }
    let paths: Vec<String> = suggestions.into_iter().map(|s| s.path).collect();
    let patch = crate::db::queries::settings::SettingsPatch {
        bucket_host_mounts_json: Some(serde_json::to_string(&paths)?),
        ..Default::default()
    };
    crate::db::queries::settings::update(db, patch).await?;
    tracing::info!(count = paths.len(), "seeded detected host toolchains into the sandbox extra-paths allowlist");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("atk-config-test-{}", uuid::Uuid::new_v4()));
        crate::db::connect(&dir.join("test.db")).await.unwrap()
    }

    /// Core rule: a brand-new install (nobody has ever touched Settings) should end up with
    /// detected toolchains already allowlisted, not an empty list waiting on manual setup.
    #[tokio::test]
    async fn seeds_detected_paths_on_a_never_touched_settings_row() {
        let db = test_pool().await;
        let home = std::env::temp_dir().join(format!("atk-config-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join(".cargo/bin")).unwrap();

        seed_detected_host_paths_at(&db, Some(&home)).await.unwrap();

        let settings = crate::db::queries::settings::get(&db).await.unwrap();
        let paths: Vec<String> = serde_json::from_str(&settings.bucket_host_mounts_json).unwrap();
        assert!(paths.iter().any(|p| p.contains(".cargo")), "expected detected cargo path, got {paths:?}");
    }

    /// A row the operator has ever saved (even to explicitly clear it back to empty) must never
    /// be silently overwritten with detected paths again.
    #[tokio::test]
    async fn never_overwrites_a_settings_row_that_has_already_been_updated() {
        let db = test_pool().await;
        let home = std::env::temp_dir().join(format!("atk-config-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(home.join(".cargo/bin")).unwrap();

        crate::db::queries::settings::update(&db, crate::db::queries::settings::SettingsPatch::default()).await.unwrap();

        seed_detected_host_paths_at(&db, Some(&home)).await.unwrap();

        let settings = crate::db::queries::settings::get(&db).await.unwrap();
        assert_eq!(settings.bucket_host_mounts_json, "[]");
    }
}
