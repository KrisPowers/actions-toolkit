pub mod cloudflare;
pub mod dashboard_guard;
pub mod dashboard_manager;
pub mod repo_listener;
pub mod repo_manager;
pub mod tailscale;

use serde::Serialize;

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
