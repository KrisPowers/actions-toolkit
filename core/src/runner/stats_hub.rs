//! `StatsHub`: fan-out for live resource-sample tails, the same shape as `log_stream::LogHub` but
//! keyed by `workflow_run_id` instead of `step_run_id`, and with no write-behind buffer — a sample
//! is already durably inserted (see `run_client::LocalRunClient::report_resource_sample`) before
//! `publish` is ever called, and the sampling cadence (a couple of seconds) is far too low-volume
//! to need batching the way chatty step logs do.

use dashmap::DashMap;
use tokio::sync::broadcast;

use atk_db::models::ResourceSample;

pub struct StatsHub {
    channels: DashMap<String, broadcast::Sender<ResourceSample>>,
}

impl StatsHub {
    pub fn new() -> Self {
        Self { channels: DashMap::new() }
    }

    pub fn subscribe(&self, workflow_run_id: &str) -> broadcast::Receiver<ResourceSample> {
        self.channels.entry(workflow_run_id.to_string()).or_insert_with(|| broadcast::channel(256).0).subscribe()
    }

    /// No-op if nobody's currently subscribed for this run (the common case — most runs finish
    /// with no live viewer), same fire-and-forget convention as `LogHub::publish`'s broadcast send.
    pub fn publish(&self, workflow_run_id: &str, sample: ResourceSample) {
        if let Some(sender) = self.channels.get(workflow_run_id) {
            let _ = sender.send(sample);
        }
    }
}

impl Default for StatsHub {
    fn default() -> Self {
        Self::new()
    }
}
