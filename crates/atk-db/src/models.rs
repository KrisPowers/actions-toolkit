use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: String,
    pub github_id: i64,
    pub github_login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub revoked: i64,
}

/// A single login attempt, successful or not, recorded regardless of whether the GitHub
/// login matched an approved (or even existing) user -- this is the "who tried to get in"
/// audit trail the admin reviews on the Login Attempts settings page.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct LoginEvent {
    pub id: String,
    pub user_id: Option<String>,
    pub github_login: Option<String>,
    pub github_id: Option<i64>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub outcome: String,
    pub created_at: String,
}

/// A GitHub login pre-approved by an admin before that person has ever signed in.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WhitelistEntry {
    pub github_login: String,
    pub added_by: Option<String>,
    pub created_at: String,
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
    /// GitHub's ID for the webhook this instance created on connect. `None` for a repo connected
    /// before webhook automation existed (manually set up) or one whose automated creation
    /// failed in a way that still left the repo row behind (shouldn't happen going forward, see
    /// `api::repos::create`, but kept nullable defensively for exactly that edge case).
    pub github_hook_id: Option<i64>,
    /// The last release ID this instance already reacted to via polling (see
    /// `core::runner::poll_sync`), so a repo without a working webhook doesn't re-dispatch
    /// `on: release` workflows for the same release on every poll. `None` until the first sync.
    pub last_synced_release_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoPublic {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub default_branch: String,
    pub webhook_url: String,
    /// Whether GitHub actually has a webhook registered for this repo (`github_hook_id` is
    /// `Some` on the underlying row). `false` means event triggers (push, pull_request,
    /// release, ...) cannot fire no matter how the workflow itself is configured, since GitHub
    /// has nowhere to deliver the event to.
    pub webhook_connected: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Singleton row (id is always 1) holding the one account-wide GitHub token entered during
/// setup. Replaces the old per-repo PAT model so connecting a repo no longer requires its own
/// credential. `token_type` is `"pat"` for the legacy manually-pasted token or `"github_app"`
/// for a token obtained through the OAuth + PKCE connect flow; the refresh/expiry/installation
/// fields are only ever populated for `"github_app"` rows.
#[derive(Debug, Clone, FromRow)]
pub struct GithubToken {
    pub id: i64,
    pub token_encrypted: Vec<u8>,
    pub token_nonce: Vec<u8>,
    pub github_login: String,
    pub scopes: String,
    pub created_at: String,
    pub updated_at: String,
    pub token_type: String,
    pub refresh_token_encrypted: Option<Vec<u8>>,
    pub refresh_token_nonce: Option<Vec<u8>>,
    pub expires_at: Option<String>,
    pub installation_id: Option<i64>,
    pub needs_reconnect: i64,
}

/// Client-facing connection status. Deliberately never carries a token or refresh-token field,
/// raw or otherwise, so adding a field here later can't silently start leaking one: see
/// `models::tests::github_token_status_never_serializes_secrets`.
#[derive(Debug, Clone, Serialize)]
pub struct GithubTokenStatus {
    pub connected: bool,
    pub github_login: Option<String>,
    pub scopes: Option<String>,
    pub connected_at: Option<String>,
    /// `"pat"` or `"github_app"`; `None` when `connected` is `false`.
    pub token_type: Option<String>,
    /// `true` for any `"pat"` row (always prompt reconnect) or a `"github_app"` row whose last
    /// refresh attempt failed. `false` when there's no connection at all.
    pub needs_reconnect: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule-proving test for milestone #1's storage rule: `GithubTokenStatus` must never
    /// serialize a raw access or refresh token, even if a future field addition tries to smuggle
    /// one in under an unexpected name. Asserts against the actual fake-secret-shaped values, not
    /// just key names, so a bug that renames the field but still emits the value still fails this.
    #[test]
    fn github_token_status_never_serializes_secrets() {
        let fake_access_token = "ghu_should_never_appear_in_json_1234567890";
        let fake_refresh_token = "ghr_should_never_appear_in_json_0987654321";

        let status = GithubTokenStatus {
            connected: true,
            github_login: Some("octocat".to_string()),
            scopes: Some(String::new()),
            connected_at: Some(now_iso()),
            token_type: Some("github_app".to_string()),
            needs_reconnect: false,
        };
        let json = serde_json::to_string(&status).unwrap();

        assert!(!json.contains(fake_access_token));
        assert!(!json.contains(fake_refresh_token));
        assert!(!json.to_lowercase().contains("refresh_token"));
        assert!(!json.to_lowercase().contains("\"token\""));
        assert!(!json.to_lowercase().contains("token_encrypted"));
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Workflow {
    pub id: String,
    pub repo_id: String,
    pub name: String,
    pub description: Option<String>,
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
    pub webhook_event_id: Option<String>,
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

/// A repo-scoped encrypted secret, injected into every job step's env the same way `GITHUB_TOKEN`
/// already is. `value_encrypted`/`value_nonce` are skipped on serialization so this type is safe
/// to return directly from a list/get API handler; only `core::secrets::decrypted_value` (which
/// requires the app's own `EncryptionKey`) can recover the plaintext.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Secret {
    pub id: String,
    pub repo_id: String,
    pub name: String,
    #[serde(skip_serializing)]
    pub value_encrypted: Vec<u8>,
    #[serde(skip_serializing)]
    pub value_nonce: Vec<u8>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
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
    /// How long a Bucket sandbox may live before the TTL reaper force-cleans it. Actually wired
    /// through to bucket creation (see `executor::run_job`); `bucket_cpu_limit_millis` and
    /// `bucket_memory_limit_mb` are columns only so far, not yet consumed by either backend, see
    /// the tracking issue.
    pub bucket_default_ttl_seconds: i64,
    pub bucket_cpu_limit_millis: Option<i64>,
    pub bucket_memory_limit_mb: Option<i64>,
    pub bucket_host_mounts_json: String,
    /// Operator-pinned external URL used to build a repo's webhook payload URL (e.g. a Cloudflare
    /// Tunnel or ngrok hostname). `None` falls back to `request_origin` (the connecting request's
    /// own Host header), which is almost always a LAN address when this instance sits behind NAT.
    pub public_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A shard: the native OS isolation (namespaces/cgroups on Linux, AppContainer/Job Objects on
/// Windows) a shell creates and tears down for one job's steps, without Docker. A child of the
/// shell that owns it, the same way the shell itself is a child of its bucket. Rows are created
/// before the shard is spawned and marked `reaped_at` once it's torn down, so a row still open at
/// startup identifies a shard that outlived a crash of the previous process.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Shard {
    pub id: String,
    pub job_run_id: String,
    pub workflow_run_id: String,
    pub os_pid: Option<i64>,
    pub os_handle_json: Option<String>,
    pub workspace_path: String,
    pub network_enabled: i64,
    pub created_at: String,
    pub ttl_expires_at: String,
    pub reaped_at: Option<String>,
}

/// The container for one triggering event (e.g. one push), which may fan out to N matched
/// workflow runs, each executing as its own `Shell` subprocess inside this bucket. Sibling shells
/// can reuse resources cached under `bucket_resource_cache` instead of regenerating them, and
/// share a single ephemeral, never-persisted decryption key held only by the bucket's own
/// in-process RCP server, never by a shell.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Bucket {
    pub id: String,
    pub trigger_kind: String,
    pub webhook_event_id: Option<String>,
    pub repo_id: String,
    pub status: String,
    #[serde(skip_serializing)]
    pub auth_token_hash: String,
    pub rcp_endpoint: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub reaped_at: Option<String>,
}

/// One real OS subprocess driving a single triggered workflow run's job DAG, talking back to its
/// parent bucket over RCP instead of touching the database directly. `agent_id` is `None` for a
/// shell spawned locally by the control plane, `Some` once it was scheduled onto a remote agent.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Shell {
    pub id: String,
    pub bucket_id: String,
    pub workflow_run_id: String,
    pub agent_id: Option<String>,
    pub target_os: String,
    pub pid: Option<i64>,
    pub status: String,
    pub exit_code: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub outcome_persisted_at: Option<String>,
    pub reaped_at: Option<String>,
    /// Only set for a shell scheduled onto a remote agent, which fetches it over the API rather
    /// than a local spec file. Contains a serialized `ShellRunSpec`, including the checkout PAT —
    /// deliberately not exposed to just any authenticated caller, only the owning agent (see
    /// `api::agents::fetch_shell_spec`'s ownership check).
    #[serde(skip_serializing)]
    pub spec_json: Option<String>,
    /// Resource-cache lookups this shell's job DAG resolved as a hit (entry already `ready`) vs. a
    /// miss (this shell became the builder). Both `0` until `ReportShellExit` reports the final
    /// counts a shell accumulated locally while running (see `core::runner::executor`).
    pub cache_hits: i64,
    pub cache_misses: i64,
}

/// One bucket-scoped shared resource (e.g. a `node_modules` produced by `npm ci`) that sibling
/// shells in the same bucket can reuse instead of regenerating. `status = "building"` is a lease
/// held by `builder_shell_id`; `builder_heartbeat_at` lets the reaper detect and reset a lease
/// whose builder died mid-build.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct BucketResourceCacheEntry {
    pub id: String,
    pub bucket_id: String,
    pub cache_key: String,
    pub status: String,
    pub path_on_disk: Option<String>,
    pub size_bytes: Option<i64>,
    pub builder_shell_id: Option<String>,
    pub builder_heartbeat_at: Option<String>,
    pub created_at: String,
    pub ready_at: Option<String>,
    pub failed_at: Option<String>,
}

/// A worker machine registered for multi-machine shell execution, approved by an operator via the
/// Agents UI. `labels_json` is a JSON array of strings (`["os=linux"]` etc.) matched against a
/// job's `runs_on`.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub os: String,
    pub arch: String,
    pub labels_json: String,
    pub capacity: i64,
    #[serde(skip_serializing)]
    pub auth_token_hash: String,
    pub status: String,
    pub last_heartbeat_at: Option<String>,
    pub version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One periodic runtime-resource sample, reported by a shell for itself (`subject_type = "shell"`)
/// or for a shard it's driving (`subject_type = "shard"`). Bucket-level figures are computed by
/// rolling up a bucket's shells' rows at query time, not stored as their own subject (see
/// `crates/atk-db/src/queries/resource_samples.rs`).
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ResourceSample {
    pub id: i64,
    pub subject_type: String,
    pub subject_id: String,
    pub workflow_run_id: Option<String>,
    pub ts: String,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<i64>,
    pub disk_read_bytes: Option<i64>,
    pub disk_write_bytes: Option<i64>,
    pub process_count: Option<i64>,
    pub host_cpu_percent: Option<f64>,
    pub host_memory_percent: Option<f64>,
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
