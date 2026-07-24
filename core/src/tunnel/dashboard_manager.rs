use std::time::Duration;

use tokio::sync::RwLock;

use super::repo_listener::ListenerHandle;
use super::{TunnelProcess, TunnelState};

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

/// The instance's own dashboard/API remote-access tunnel (Decision 3): a singleton, entirely
/// separate from every repo's webhook tunnel (`tunnel::repo_manager::RepoTunnelManager`) -- its
/// own `TunnelProcess`, its own loopback listener, its own hardened router, never sharing a
/// process or port with any of them.
pub struct DashboardTunnelManager {
    process: std::sync::Arc<TunnelProcess>,
    listener: RwLock<Option<ListenerHandle>>,
    pub ip_limiter: atk_auth::rate_limit::RateLimiter,
    pub user_limiter: atk_auth::rate_limit::RateLimiter,
}

impl DashboardTunnelManager {
    pub fn new() -> Self {
        Self {
            process: std::sync::Arc::new(TunnelProcess::new()),
            listener: RwLock::new(None),
            ip_limiter: atk_auth::rate_limit::RateLimiter::with_capacity_bound(IP_MAX_REQUESTS_PER_MINUTE, Duration::from_secs(60), MAX_TRACKED_KEYS),
            user_limiter: atk_auth::rate_limit::RateLimiter::with_capacity_bound(USER_MAX_REQUESTS_PER_MINUTE, Duration::from_secs(60), MAX_TRACKED_KEYS),
        }
    }

    pub async fn status(&self) -> TunnelState {
        self.process.status().await
    }
}

impl Default for DashboardTunnelManager {
    fn default() -> Self {
        Self::new()
    }
}
