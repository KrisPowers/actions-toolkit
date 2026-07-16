use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub revoked: i64,
}

#[derive(Debug, Clone, FromRow)]
pub struct Repo {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub webhook_secret_encrypted: Vec<u8>,
    pub webhook_secret_nonce: Vec<u8>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoPublic {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub webhook_url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Singleton row (id is always 1) holding the one account-wide GitHub token entered during
/// setup. Replaces the old per-repo PAT model so connecting a repo no longer requires its own
/// credential.
#[derive(Debug, Clone, FromRow)]
pub struct GithubToken {
    pub id: i64,
    pub token_encrypted: Vec<u8>,
    pub token_nonce: Vec<u8>,
    pub github_login: String,
    pub scopes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GithubTokenStatus {
    pub connected: bool,
    pub github_login: Option<String>,
    pub scopes: Option<String>,
    pub connected_at: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Workflow {
    pub id: String,
    pub repo_id: String,
    pub name: String,
    pub file_path: String,
    pub yaml_source: String,
    pub parsed_json: String,
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_id: String,
    pub repo_id: String,
    pub trigger_event: String,
    pub trigger_payload_json: Option<String>,
    pub ref_name: Option<String>,
    pub commit_sha: Option<String>,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct JobRun {
    pub id: String,
    pub workflow_run_id: String,
    pub job_key: String,
    pub name: Option<String>,
    pub status: String,
    pub needs_json: String,
    pub container_id: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct StepRun {
    pub id: String,
    pub job_run_id: String,
    pub step_index: i64,
    pub name: Option<String>,
    pub kind: String,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RunLog {
    pub id: i64,
    pub step_run_id: String,
    pub ts: String,
    pub stream: String,
    pub message: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Artifact {
    pub id: String,
    pub workflow_run_id: String,
    pub job_run_id: Option<String>,
    pub name: String,
    pub path_on_disk: String,
    pub size_bytes: i64,
    pub content_type: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WebhookEvent {
    pub id: String,
    pub repo_id: Option<String>,
    pub github_event: String,
    pub delivery_id: Option<String>,
    pub payload_json: String,
    pub signature_valid: i64,
    pub matched_workflow_ids: String,
    pub received_at: String,
}

/// Singleton row (id is always 1) holding runtime settings that used to be CLI/.env-only.
/// Seeded with defaults by `migrations/0010_settings.sql`, so it always exists once the
/// database has been created.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Settings {
    pub id: i64,
    pub port: i64,
    pub bind_addr: String,
    pub docker_host: Option<String>,
    pub max_concurrent_jobs: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

pub fn parse_iso(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[derive(Debug, Clone, Serialize)]
pub struct RunTree {
    pub run: WorkflowRun,
    pub jobs: Vec<JobRunTree>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobRunTree {
    pub job: JobRun,
    pub steps: Vec<StepRun>,
}
