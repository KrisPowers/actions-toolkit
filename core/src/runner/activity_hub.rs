//! `ActivityHub`: fan-out for "a new workflow run was just created for this repo" notifications,
//! the same shape as `log_stream::LogHub`/`stats_hub::StatsHub` but keyed by `repo_id`. Lets the
//! Overview page's run list update the instant a new trigger fires (push, PR, manual dispatch,
//! ...) instead of waiting on the next poll.

use dashmap::DashMap;
use tokio::sync::broadcast;

use atk_db::models::WorkflowRun;

pub struct ActivityHub {
    channels: DashMap<String, broadcast::Sender<WorkflowRun>>,
}

impl ActivityHub {
    pub fn new() -> Self {
        Self { channels: DashMap::new() }
    }

    pub fn subscribe(&self, repo_id: &str) -> broadcast::Receiver<WorkflowRun> {
        self.channels.entry(repo_id.to_string()).or_insert_with(|| broadcast::channel(64).0).subscribe()
    }

    /// No-op if nobody's currently watching this repo's Overview page — the common case.
    pub fn publish(&self, repo_id: &str, run: WorkflowRun) {
        if let Some(sender) = self.channels.get(repo_id) {
            let _ = sender.send(run);
        }
    }
}

impl Default for ActivityHub {
    fn default() -> Self {
        Self::new()
    }
}
