pub mod cloudflare;
pub mod dashboard_guard;
pub mod dashboard_manager;
pub mod repo_listener;
pub mod repo_manager;
pub mod tailscale;

use serde::Serialize;
use tokio::process::Child;
use tokio::sync::{Mutex, RwLock};

/// One tunnel provider's process state, shared by every tunnel this instance ever runs, whether
/// it's a specific repo's webhook tunnel or the instance's own dashboard tunnel: each is its own
/// independent `TunnelProcess`, never shared between two different tunnels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TunnelState {
    Idle,
    Starting,
    Running { url: String },
    Failed { message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelProvider {
    Cloudflare,
    Tailscale,
}

impl TunnelProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            TunnelProvider::Cloudflare => "cloudflare",
            TunnelProvider::Tailscale => "tailscale",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cloudflare" => Some(TunnelProvider::Cloudflare),
            "tailscale" => Some(TunnelProvider::Tailscale),
            _ => None,
        }
    }
}

/// A single tunnel process (either `cloudflared` or `tailscale funnel`), tracked independently of
/// any other tunnel. Whether this backs a specific repo's webhook tunnel or the instance's
/// dashboard tunnel, its lifecycle (start/stop/state) never affects any other `TunnelProcess`.
pub struct TunnelProcess {
    state: RwLock<TunnelState>,
    child: Mutex<Option<Child>>,
}

impl TunnelProcess {
    pub fn new() -> Self {
        Self { state: RwLock::new(TunnelState::Idle), child: Mutex::new(None) }
    }

    pub async fn status(&self) -> TunnelState {
        self.state.read().await.clone()
    }
}

impl Default for TunnelProcess {
    fn default() -> Self {
        Self::new()
    }
}
