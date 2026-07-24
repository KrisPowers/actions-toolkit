use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::app::AppState;
use crate::db::queries::repo_tunnels as repo_tunnels_queries;

use super::repo_listener::{self, ListenerHandle};
use super::{TunnelProcess, TunnelProvider, TunnelState};

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

    /// Starts (or, if already running under the same provider, no-ops) repo_id's tunnel: ensures
    /// a loopback listener serving only that repo's webhook route exists, then spawns the tunnel
    /// process pointed at it. Persists the choice so a restart of the whole instance can bring it
    /// back automatically (see `repo_tunnels::list_enabled` and the boot-time loop in `main.rs`).
    pub async fn start(&self, state: &AppState, repo_id: &str, provider: TunnelProvider) -> anyhow::Result<()> {
        let process = {
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get(repo_id) {
                entry.process.clone()
            } else {
                let listener = repo_listener::spawn(state.clone(), repo_id.to_string()).await?;
                let process = Arc::new(TunnelProcess::new());
                entries.insert(repo_id.to_string(), RepoTunnelEntry { process: process.clone(), listener });
                process
            }
        };
        let local_port = self.entries.read().await.get(repo_id).expect("just inserted above").listener.local_port;

        super::start(process, provider, local_port).await;
        repo_tunnels_queries::upsert(&state.db, repo_id, provider.as_str(), true, Some(local_port as i64), None).await?;
        Ok(())
    }
}
