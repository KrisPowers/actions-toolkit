//! Bridges `atk_bucket::PhaseRecorder`'s sync callbacks (fired from deep inside blocking OS code
//! -- see that trait's own doc comment for why they're sync) into `RunClient`'s async
//! `start_phase`/`finish_phase`. Every write here is fire-and-forget (`tokio::spawn`, never
//! awaited), the same tradeoff `executor::run_job`'s `on_line` closure already makes: the sync
//! caller has no way to await anything, and a lost trip-wire write is a far better outcome than
//! blocking (or failing) the actual sandbox operation it's only watching.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use atk_bucket::PhaseRecorder;

use crate::db::models::now_iso;
use crate::runner::run_client::RunClient;

pub struct RunClientPhaseRecorder {
    run_client: Arc<dyn RunClient>,
    subject_type: &'static str,
    subject_id: String,
    workflow_run_id: Option<String>,
    // Keyed by phase name, not a single slot: `create_job_shard`'s recorder sees several
    // distinctly-named phases in flight at once (though never the *same* name twice concurrently
    // -- a phase that can repeat per call, like one ACL grant per ancestor, folds its
    // disambiguating detail into the phase name itself, see `atk_bucket::PhaseRecorder`'s doc).
    // The id itself is generated synchronously in `start`, before the actual DB write is handed
    // off to a detached task, so `finish` always has something to look up even though the row
    // `start` opens may not have landed yet.
    open: Mutex<HashMap<String, String>>,
}

impl RunClientPhaseRecorder {
    pub fn new(run_client: Arc<dyn RunClient>, subject_type: &'static str, subject_id: String, workflow_run_id: Option<String>) -> Self {
        Self { run_client, subject_type, subject_id, workflow_run_id, open: Mutex::new(HashMap::new()) }
    }
}

impl PhaseRecorder for RunClientPhaseRecorder {
    fn start(&self, phase: &str, detail: Option<&str>) {
        let id = uuid::Uuid::new_v4().to_string();
        self.open.lock().unwrap().insert(phase.to_string(), id.clone());

        let run_client = self.run_client.clone();
        let phase = phase.to_string();
        let subject_type = self.subject_type;
        let subject_id = self.subject_id.clone();
        let workflow_run_id = self.workflow_run_id.clone();
        let detail = detail.map(str::to_string);
        let started_at = now_iso();
        tokio::spawn(async move {
            let _ = run_client
                .start_phase(&id, &phase, subject_type, &subject_id, workflow_run_id.as_deref(), detail.as_deref(), &started_at)
                .await;
        });
    }

    fn finish(&self, phase: &str, ok: bool, detail: Option<&str>) {
        let Some(id) = self.open.lock().unwrap().remove(phase) else { return };
        let run_client = self.run_client.clone();
        let detail = detail.map(str::to_string);
        tokio::spawn(async move {
            let _ = run_client.finish_phase(&id, Some(ok), detail.as_deref()).await;
        });
    }
}
