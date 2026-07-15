use std::sync::Arc;

use axum::extract::FromRef;
use bollard::Docker;
use dashmap::DashMap;
use sqlx::SqlitePool;

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
    pub github_clients: DashMap<String, octocrab::Octocrab>,
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
