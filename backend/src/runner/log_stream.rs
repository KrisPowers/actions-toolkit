use dashmap::DashMap;
use serde::Serialize;
use sqlx::SqlitePool;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

use crate::db::queries::runs as run_queries;

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub step_run_id: String,
    pub ts: String,
    pub stream: String, // "stdout" | "stderr" | "system"
    pub message: String,
}

const BATCH_SIZE: usize = 50;
const BATCH_MS: u64 = 100;

/// Fan-out hub: one broadcast channel per actively-running step, plus a small write-behind
/// buffer so chatty build output doesn't issue one SQLite INSERT per log line.
pub struct LogHub {
    channels: DashMap<String, broadcast::Sender<LogLine>>,
    buffer: Mutex<Vec<(String, String, String, String)>>,
}

impl LogHub {
    pub fn new() -> Self {
        Self {
            channels: DashMap::new(),
            buffer: Mutex::new(Vec::new()),
        }
    }

    pub fn subscribe(&self, step_run_id: &str) -> broadcast::Receiver<LogLine> {
        self.channels
            .entry(step_run_id.to_string())
            .or_insert_with(|| broadcast::channel(1024).0)
            .subscribe()
    }

    pub async fn publish(&self, pool: &SqlitePool, line: LogLine) {
        if let Some(sender) = self.channels.get(&line.step_run_id) {
            let _ = sender.send(line.clone());
        }

        let should_flush = {
            let mut buf = self.buffer.lock().await;
            buf.push((line.step_run_id, line.ts, line.stream, line.message));
            buf.len() >= BATCH_SIZE
        };
        if should_flush {
            self.flush(pool).await;
        }
    }

    pub async fn flush(&self, pool: &SqlitePool) {
        let batch = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };
        if let Err(e) = run_queries::insert_log_lines(pool, &batch).await {
            tracing::error!(error = %e, "failed to flush log batch");
        }
    }

    pub fn close(&self, step_run_id: &str) {
        self.channels.remove(step_run_id);
    }

    /// Background task: flush the buffer on a fixed interval so short-lived steps still get
    /// their trailing log lines persisted promptly.
    pub async fn run_periodic_flush(hub: std::sync::Arc<Self>, pool: SqlitePool) {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(BATCH_MS));
        loop {
            interval.tick().await;
            hub.flush(&pool).await;
        }
    }
}

impl Default for LogHub {
    fn default() -> Self {
        Self::new()
    }
}
