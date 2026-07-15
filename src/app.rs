use std::sync::Arc;

use axum::extract::FromRef;
use bollard::Docker;
use octocrab::Octocrab;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::auth::jwt::JwtCodec;
use crate::config::AppConfig;
use crate::crypto::EncryptionKey;
use crate::runner::log_stream::LogHub;

#[derive(Clone)]
pub struct AppState(pub Arc<AppStateInner>);

pub struct AppStateInner {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub jwt: JwtCodec,
    pub enc: EncryptionKey,
    /// `None` when the Docker Engine could not be reached at startup; workflow dispatch
    /// endpoints return a clear error instead of panicking when this is absent.
    pub docker: Option<Docker>,
    pub log_hub: Arc<LogHub>,
    /// Cached client for the single account-wide GitHub token set up in the setup wizard.
    /// `None` until a token has been configured, or after `github::client::invalidate` runs
    /// following a rotation/removal.
    pub github_client: RwLock<Option<Octocrab>>,
}

impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.0.db.clone()
    }
}

impl std::ops::Deref for AppState {
    type Target = AppStateInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
