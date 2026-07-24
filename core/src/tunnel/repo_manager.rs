use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::repo_listener::ListenerHandle;
use super::{TunnelProcess, TunnelState};

struct RepoTunnelEntry {
    process: Arc<TunnelProcess>,
    listener: ListenerHandle,
}

/// Tracks every repo's webhook tunnel independently, keyed by repo_id. Each entry owns its own
/// child process (via its `TunnelProcess`) and its own loopback listener (`repo_listener::spawn`)
/// serving only that repo's webhook route -- starting, stopping, or tearing down one repo's entry
/// never touches any other repo's, and no repo's tunnel is ever shared with another's.
#[derive(Default)]
pub struct RepoTunnelManager {
    entries: RwLock<HashMap<String, RepoTunnelEntry>>,
}

impl RepoTunnelManager {
    pub fn new() -> Self {
        Self { entries: RwLock::new(HashMap::new()) }
    }

    pub async fn status(&self, repo_id: &str) -> TunnelState {
        match self.entries.read().await.get(repo_id) {
            Some(entry) => entry.process.status().await,
            None => TunnelState::Idle,
        }
    }
}
