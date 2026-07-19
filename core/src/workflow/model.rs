use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Canonical in-memory representation of a workflow. Both the YAML code editor and the
/// visual (React Flow) builder read and write this same shape via JSON; the backend owns
/// the JSON<->YAML conversion so there is exactly one source of truth for serialization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workflow {
    pub name: String,
    #[serde(default)]
    pub on: TriggerConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<IndexMap<String, String>>,
    #[serde(default)]
    pub jobs: IndexMap<String, Job>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TriggerConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push: Option<PushTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "pull_request")]
    pub pull_request: Option<PullRequestTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issues: Option<IssuesTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "workflow_dispatch")]
    pub workflow_dispatch: Option<ManualTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Vec<CronTrigger>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PushTrigger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrEventType {
    Opened,
    Synchronize,
    Reopened,
    Closed,
    Labeled,
    Unlabeled,
    ReadyForReview,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PullRequestTrigger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<PrEventType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branches: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEventType {
    Published,
    Created,
    Edited,
    Deleted,
    Prereleased,
    Released,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseTrigger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<ReleaseEventType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuesEventType {
    Opened,
    Edited,
    Closed,
    Reopened,
    Labeled,
    Unlabeled,
    Assigned,
    Unassigned,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IssuesTrigger {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<IssuesEventType>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManualTrigger {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<IndexMap<String, WorkflowInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTrigger {
    pub cron: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_runs_on")]
    pub runs_on: String,
    /// `run:` steps execute inside this container (via Docker) when set; when unset they run
    /// natively via the Bucket sandbox instead, matching real GitHub Actions' own default
    /// (no `container:` key means the job runs directly on the runner, not inside a container).
    /// `uses: docker://` steps always use their own one-off container regardless of this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "if")]
    pub if_condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Strategy>,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", rename = "download_artifacts")]
    pub download_artifacts: Vec<String>,
    /// Opts this job's sandbox into network access. Bucket (the native, non-Docker sandbox
    /// backend) is network-default-deny; Docker's own container networking is unaffected by
    /// this and keeps working as before regardless of its value.
    #[serde(default)]
    pub network: bool,
}

fn default_runs_on() -> String {
    "self-hosted".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSpec {
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<IndexMap<String, Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uses: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub with: Option<IndexMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "if")]
    pub if_condition: Option<String>,
    #[serde(default, rename = "continue-on-error")]
    pub continue_on_error: bool,
    /// Overrides the shell a `run:` step's command is executed with (`bash`, `sh`, `pwsh`,
    /// `powershell`, `cmd`). Each backend (Docker exec, the Linux Bucket sandbox, the Windows
    /// Bucket sandbox) resolves its own platform-appropriate default when this is unset, mirroring
    /// real GitHub Actions rather than a single shell hardcoded across every platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

impl Step {
    pub fn kind(&self) -> &'static str {
        if self.uses.as_deref().is_some_and(|u| u.starts_with("docker://")) {
            "uses_docker"
        } else if self.uses.as_deref() == Some("checkout") {
            "checkout"
        } else {
            "run"
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSpec {
    pub name: String,
    pub path: String,
}
