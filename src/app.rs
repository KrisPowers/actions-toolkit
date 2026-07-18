use std::sync::Arc;

use axum::extract::FromRef;
use bollard::Docker;
use dashmap::DashMap;
use octocrab::Octocrab;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::auth::jwt::JwtCodec;
use crate::config::AppConfig;
use crate::crypto::EncryptionKey;
use crate::github::oauth::PendingAuthorize;
use crate::runner::log_stream::LogHub;

#[derive(Clone)]
pub struct AppState(pub Arc<AppStateInner>);

pub struct AppStateInner {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub jwt: JwtCodec,
    pub enc: EncryptionKey,
    /// `None` when the Docker Engine could not be reached at startup. `run:` steps no longer
    /// need this (they run via Bucket by default); it's still required for jobs that declare
    /// `container:` and for `uses: docker://` steps, which fail with a clear error rather than
    /// panicking when this is absent.
    pub docker: Option<Docker>,
    /// Whether Bucket (the native, non-Docker sandbox) actually works on this host, probed once
    /// at startup. `run:` steps in jobs without a `container:` need this; a scheduler run only
    /// hard-fails up front when this is `false`, since that means no job lacking `container:` can
    /// execute at all, unlike a missing Docker connection which only affects specific jobs/steps.
    pub bucket_capability_ok: bool,
    pub log_hub: Arc<LogHub>,
    /// Cached client for the single account-wide GitHub token set up in the setup wizard.
    /// `None` until a token has been configured, or after `github::client::invalidate` runs
    /// following a rotation/removal.
    pub github_client: RwLock<Option<Octocrab>>,
    /// In-flight OAuth authorize attempts (PKCE verifier), keyed by the CSRF state value, for
    /// `/auth/github/authorize` and `/auth/github/callback`. Entries are removed the moment a
    /// callback consumes them and swept for staleness on each new authorize call, so this stays
    /// bounded even if a user starts a connect flow and never finishes it.
    pub oauth_states: DashMap<String, PendingAuthorize>,
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
