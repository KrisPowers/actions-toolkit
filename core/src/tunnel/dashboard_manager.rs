use std::time::Duration;

use tokio::sync::{Mutex, RwLock};

use crate::app::AppState;
use crate::db::queries::dashboard_tunnel as dashboard_tunnel_queries;
use crate::db::queries::dashboard_tunnel_requests::{self as requests_queries, NewRequest};

use super::repo_listener::{self, ListenerHandle};
use super::{TunnelProcess, TunnelProvider, TunnelState};

/// Requests over the limit within a 60s window per source IP, before any session even exists --
/// a blunt backstop for pre-auth traffic (the login poll, `/auth/status`, etc).
const IP_MAX_REQUESTS_PER_MINUTE: u32 = 120;
/// Requests over the limit within a 60s window per (IP, user) pair once a session cookie is
/// present -- generous enough for normal dashboard polling/WS reconnects/multiple tabs, tight
/// enough to blunt scripted abuse. A starting point to tune after real dogfooding, not a proven
/// final value.
const USER_MAX_REQUESTS_PER_MINUTE: u32 = 300;
/// Bounds how many distinct rate-limit keys are ever tracked at once, so an attacker sprayed
/// across many source IPs can't grow the map without bound (see `atk_auth::rate_limit`'s doc
/// comment on why the login-only limiter's "never evict" assumption doesn't hold here).
const MAX_TRACKED_KEYS: usize = 10_000;

const AUDIT_FLUSH_INTERVAL: Duration = Duration::from_secs(5);
/// Above this many buffered entries, flush immediately rather than waiting for the next tick, so
/// a burst can't grow the in-memory buffer unbounded between ticks.
const AUDIT_FLUSH_BATCH_SIZE: usize = 200;

/// The instance's own dashboard/API remote-access tunnel (Decision 3): a singleton, entirely
/// separate from every repo's webhook tunnel (`tunnel::repo_manager::RepoTunnelManager`) -- its
/// own `TunnelProcess`, its own loopback listener, its own hardened router, never sharing a
/// process or port with any of them.
pub struct DashboardTunnelManager {
    process: std::sync::Arc<TunnelProcess>,
    listener: RwLock<Option<ListenerHandle>>,
    pub ip_limiter: atk_auth::rate_limit::RateLimiter,
    pub user_limiter: atk_auth::rate_limit::RateLimiter,
    audit_buffer: Mutex<Vec<NewRequest>>,
}

impl DashboardTunnelManager {
    pub fn new() -> Self {
        Self {
            process: std::sync::Arc::new(TunnelProcess::new()),
            listener: RwLock::new(None),
            ip_limiter: atk_auth::rate_limit::RateLimiter::with_capacity_bound(IP_MAX_REQUESTS_PER_MINUTE, Duration::from_secs(60), MAX_TRACKED_KEYS),
            user_limiter: atk_auth::rate_limit::RateLimiter::with_capacity_bound(USER_MAX_REQUESTS_PER_MINUTE, Duration::from_secs(60), MAX_TRACKED_KEYS),
            audit_buffer: Mutex::new(Vec::new()),
        }
    }

    pub async fn status(&self) -> TunnelState {
        self.process.status().await
    }

    /// Ensures the hardened dashboard-tunnel listener exists (building it from
    /// `api::dashboard_routes()` plus the `dashboard_guard` middleware layer, entirely separate
    /// from the plain LAN router), then starts the tunnel process pointed at it. Persists the
    /// choice so a restart of the whole instance can bring it back automatically.
    pub async fn start(&self, state: &AppState, provider: TunnelProvider) -> anyhow::Result<()> {
        let local_port = {
            let mut listener = self.listener.write().await;
            if listener.is_none() {
                let router = crate::api::dashboard_routes()
                    .layer(axum::middleware::from_fn_with_state(state.clone(), super::dashboard_guard::guard))
                    .layer(tower_http::trace::TraceLayer::new_for_http())
                    .with_state(state.clone());
                *listener = Some(repo_listener::spawn_with_router(router).await?);
            }
            listener.as_ref().expect("just ensured above").local_port
        };

        super::start(self.process.clone(), provider, local_port).await;
        dashboard_tunnel_queries::update(&state.db, provider.as_str(), true, Some(local_port as i64), None).await?;
        Ok(())
    }

    /// Stops the tunnel process but leaves the loopback listener bound, so a subsequent `start`
    /// can reuse the same local port instead of allocating a new one.
    pub async fn stop(&self, state: &AppState) -> anyhow::Result<()> {
        super::stop(&self.process).await;
        let current = dashboard_tunnel_queries::get(&state.db).await?;
        dashboard_tunnel_queries::update(&state.db, &current.provider, false, current.local_port, current.last_url.as_deref()).await?;
        Ok(())
    }

    /// Buffers one request for the periodic flush (see `run_periodic_flush`), rather than issuing
    /// a write per request, so a request flood can't turn into a write storm on top of the
    /// traffic itself. Flushes immediately, using `pool`, once the buffer crosses
    /// `AUDIT_FLUSH_BATCH_SIZE` rather than waiting for the next periodic tick.
    #[allow(clippy::too_many_arguments)]
    pub async fn record_request(
        &self,
        pool: &sqlx::SqlitePool,
        ip_address: Option<String>,
        user_id: Option<String>,
        method: String,
        path: String,
        status_code: u16,
        rate_limited: bool,
    ) {
        let should_flush_now = {
            let mut buf = self.audit_buffer.lock().await;
            buf.push(NewRequest { user_id, ip_address, method, path, status_code: status_code as i64, rate_limited });
            buf.len() >= AUDIT_FLUSH_BATCH_SIZE
        };
        if should_flush_now {
            self.flush(pool).await;
        }
    }

    async fn flush(&self, pool: &sqlx::SqlitePool) {
        let batch = {
            let mut buf = self.audit_buffer.lock().await;
            std::mem::take(&mut *buf)
        };
        if let Err(e) = requests_queries::record_batch(pool, &batch).await {
            tracing::error!(error = %e, "failed to flush dashboard tunnel request audit batch");
        }
    }
}

impl Default for DashboardTunnelManager {
    fn default() -> Self {
        Self::new()
    }
}
