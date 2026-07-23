//! Wire types for RCP ("Run Control Protocol"): what a shell sends its owning bucket instead of
//! touching the database directly. `atk_rcp` only provides the generic framing/transport; these
//! types are what actually gets framed, since they need `core`'s own DTOs, not `atk-db`'s row
//! types directly, so the wire shape doesn't have to change every time a DB column does.

use serde::{Deserialize, Serialize};

/// First message on every RCP connection. `bucket_id` selects which bucket's endpoint this is
/// (defensive: a shell only ever dials its own bucket's endpoint anyway) and `auth_token` proves
/// the connecting process is a shell this bucket actually spawned, not just anything with local
/// access to the pipe/socket.
#[derive(Debug, Serialize, Deserialize)]
pub struct Hello {
    pub bucket_id: String,
    pub auth_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RcpRequest {
    SetRunStatus { workflow_run_id: String, status: String, terminal: bool },
    SetJobStatus { job_run_id: String, status: String, exit_code: Option<i64>, terminal: bool },
    SetJobContainer { job_run_id: String, container_id: String },
    CreateStepRun { job_run_id: String, step_index: i64, name: Option<String>, kind: String },
    SetStepStatus { step_run_id: String, status: String, exit_code: Option<i64>, terminal: bool },
    InsertLogLine { step_run_id: String, ts: String, stream: String, message: String },
    FindArtifactByRunAndName { workflow_run_id: String, name: String },
    RecordArtifact { workflow_run_id: String, job_run_id: Option<String>, name: String, path_on_disk: String, size_bytes: i64 },
    ListSecretNames,
    RequestSecret { name: String },
    ResourceCacheLookup { cache_key: String },
    ResourceCacheBeginBuild { cache_key: String, shell_id: String },
    ResourceCacheHeartbeat { entry_id: String },
    ResourceCacheComplete { entry_id: String, path_on_disk: String, size_bytes: i64 },
    ResourceCacheFail { entry_id: String },
    RecordJobShard { id: String, job_run_id: String, workflow_run_id: String, workspace_path: String, network_enabled: bool, ttl_expires_at: String },
    MarkShardReaped { shard_id: String },
    ReportShellExit { shell_id: String, exit_code: i64, cache_hits: i64, cache_misses: i64 },
    ReportResourceSample {
        subject_type: String,
        subject_id: String,
        workflow_run_id: Option<String>,
        ts: String,
        cpu_percent: Option<f64>,
        memory_bytes: Option<i64>,
        disk_read_bytes: Option<i64>,
        disk_write_bytes: Option<i64>,
        process_count: Option<i64>,
        host_cpu_percent: Option<f64>,
        host_memory_percent: Option<f64>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub path_on_disk: String,
}

/// `is_builder` is only meaningful as the answer to `ResourceCacheBeginBuild`: `true` means this
/// shell's call is the one that actually created the `building` row and must run the real build;
/// `false` means another shell already holds the lease and the caller should poll `status`
/// instead. A plain `ResourceCacheLookup` always returns `is_builder: false`, since a lookup never
/// claims anything.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceCacheState {
    pub entry_id: String,
    pub status: String,
    pub path_on_disk: Option<String>,
    pub is_builder: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum RcpResponse {
    Ok,
    StepRunId(String),
    Artifact(Option<ArtifactInfo>),
    SecretNames(Vec<String>),
    Secret(Option<String>),
    ResourceCache(ResourceCacheState),
    Error(String),
}
