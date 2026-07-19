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
