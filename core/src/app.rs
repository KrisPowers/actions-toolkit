use std::sync::Arc;

use axum::extract::FromRef;
use bollard::Docker;
use octocrab::Octocrab;
use sqlx::SqlitePool;
use tokio::sync::RwLock;

use crate::auth::jwt::JwtCodec;
use crate::config::AppConfig;
use crate::crypto::EncryptionKey;
use crate::github::oauth::{DeviceFlowResult, PendingDeviceFlow};
use crate::runner::activity_hub::ActivityHub;
use crate::runner::log_stream::LogHub;
use crate::runner::stats_hub::StatsHub;

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
    /// Why Bucket isn't available, when `bucket_capability_ok` is `false`; `None` either when
    /// it's available or (rare) when the probe failed without a specific reason.
    pub bucket_capability_reason: Option<String>,
    pub log_hub: Arc<LogHub>,
    /// Fan-out for live runtime-resource sample tails (CPU/memory/disk/process count), the same
    /// role `log_hub` plays for step output — see `runner::stats_hub` and the `/runs/:id/stats/ws`
    /// route.
    pub stats_hub: Arc<StatsHub>,
    /// Fan-out for "a new workflow run was just created for this repo" — see `runner::activity_hub`
    /// and the `/repos/:id/activity/ws` route, the Overview page's live run list.
    pub activity_hub: Arc<ActivityHub>,
    /// Cached client for the single account-wide GitHub token set up in the setup wizard.
    /// `None` until a token has been configured, or after `github::client::invalidate` runs
    /// following a rotation/removal.
    pub github_client: RwLock<Option<Octocrab>>,
    /// The in-flight device-flow connect attempt, if any (`/auth/github/device/start` sets it,
    /// `/auth/github/device/poll` consumes it). At most one at a time: a single-operator,
    /// single-instance tool never has two connect attempts in flight together, so a new `start`
    /// simply replaces whatever was here.
    pub pending_device_flow: RwLock<Option<PendingDeviceFlow>>,
    /// Set once the server-side poller (spawned by `device_start`) sees a terminal outcome for
    /// the current `pending_device_flow` attempt. `/auth/github/device/poll` just reads this
    /// rather than calling GitHub itself, so the attempt still completes (and, on success, gets
    /// persisted) even if no browser tab is polling at the moment GitHub reports success.
    pub device_flow_result: RwLock<Option<DeviceFlowResult>>,
    /// Serializes the GitHub App access-token refresh in `github::client::ensure_fresh_app_token`.
    /// GitHub App refresh tokens are single-use: two callers racing to refresh the same stale
    /// token at once (e.g. two workflow runs checking out code at the same moment) would have the
    /// loser's exchange rejected by GitHub, wrongly marking a connection that the winner just
    /// refreshed fine as needing reconnect.
    pub token_refresh_lock: tokio::sync::Mutex<()>,
    /// The instance-wide "one click" Cloudflare Quick Tunnel started from the Webhooks page.
    /// Shared (not per-repo): a tunnel exposes this instance's port, not any single repo.
    pub cloudflare_tunnel: Arc<crate::tunnel::CloudflareTunnel>,
    /// The instance-wide "one click" Tailscale Funnel started from the Webhooks page. Same
    /// shared-not-per-repo reasoning as `cloudflare_tunnel`.
    pub tailscale_tunnel: Arc<crate::tailscale::TailscaleTunnel>,
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
