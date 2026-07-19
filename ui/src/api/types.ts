export interface User {
  id: string;
  username: string;
  role: string;
}

export interface RepoPublic {
  id: string;
  owner: string;
  name: string;
  default_branch: string;
  webhook_url: string;
  webhook_connected: boolean;
  created_at: string;
  updated_at: string;
}

export interface GithubTokenStatus {
  connected: boolean;
  github_login: string | null;
  scopes: string | null;
  connected_at: string | null;
  token_type: "pat" | "github_app" | null;
  needs_reconnect: boolean;
}

export interface DeviceStartResponse {
  user_code: string;
  verification_uri: string;
  interval: number;
  expires_in: number;
}

export type DevicePollResponse =
  | { status: "pending" }
  | { status: "denied" }
  | { status: "expired" }
  | { status: "not_started" }
  | { status: "connected"; github_login: string; has_installation: boolean };

export interface Settings {
  id: number;
  port: number;
  bind_addr: string;
  docker_host: string | null;
  max_concurrent_jobs: number;
  bucket_default_ttl_seconds: number;
  bucket_cpu_limit_millis: number | null;
  bucket_memory_limit_mb: number | null;
  bucket_host_mounts_json: string;
  created_at: string;
  updated_at: string;
}

export interface RuntimeStatus {
  docker_available: boolean;
  bucket_available: boolean;
  bucket_unavailable_reason: string | null;
}

export interface UpdateSettingsRequest {
  bind_addr?: string;
  docker_host?: string;
  max_concurrent_jobs?: number;
  bucket_default_ttl_seconds?: number;
  bucket_cpu_limit_millis?: number;
  bucket_memory_limit_mb?: number;
  bucket_host_mounts_json?: string;
}

export interface AccessibleRepo {
  owner: string;
  name: string;
  full_name: string;
  private: boolean;
  default_branch: string;
}

export interface WorkflowRow {
  id: string;
  repo_id: string;
  name: string;
  description: string | null;
  file_path: string;
  yaml_source: string;
  parsed_json: string;
  enabled: number;
  created_at: string;
  updated_at: string;
}

export interface GithubWorkflowFile {
  name: string;
  path: string;
  sha: string;
}

export type RunStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export type JobStatus = "pending" | "queued" | "running" | "succeeded" | "failed" | "skipped" | "cancelled";
export type StepStatus = JobStatus;

export interface WorkflowRun {
  id: string;
  workflow_id: string;
  repo_id: string;
  trigger_event: string;
  trigger_payload_json: string | null;
  ref_name: string | null;
  commit_sha: string | null;
  status: RunStatus;
  started_at: string | null;
  finished_at: string | null;
  created_at: string;
}

export interface JobRun {
  id: string;
  workflow_run_id: string;
  job_key: string;
  name: string | null;
  status: JobStatus;
  needs_json: string;
  container_id: string | null;
  started_at: string | null;
  finished_at: string | null;
  exit_code: number | null;
}

export interface StepRun {
  id: string;
  job_run_id: string;
  step_index: number;
  name: string | null;
  kind: string;
  status: StepStatus;
  started_at: string | null;
  finished_at: string | null;
  exit_code: number | null;
}

export interface JobRunTree {
  job: JobRun;
  steps: StepRun[];
}

export interface RunTree {
  run: WorkflowRun;
  jobs: JobRunTree[];
}

export interface RunLog {
  id: number;
  step_run_id: string;
  ts: string;
  stream: "stdout" | "stderr" | "system";
  message: string;
}

export interface Artifact {
  id: string;
  workflow_run_id: string;
  job_run_id: string | null;
  name: string;
  path_on_disk: string;
  size_bytes: number;
  content_type: string | null;
  created_at: string;
}

export interface ArtifactWithContext extends Artifact {
  workflow_name: string;
  run_status: RunStatus;
  run_created_at: string;
}

// --- GitHub REST models (subset), proxied through by src/github/issues.rs and
// src/github/releases.rs via octocrab, which serializes GitHub's REST API shape directly. ---

export interface GithubUser {
  login: string;
  avatar_url: string;
}

export interface GithubLabel {
  name: string;
  color: string;
}

export interface GithubIssue {
  id: number;
  number: number;
  title: string;
  state: "open" | "closed";
  user: GithubUser | null;
  labels: GithubLabel[];
  pull_request?: unknown;
  created_at: string;
  updated_at: string;
}

export interface GithubPullRequestRef {
  ref: string;
  sha: string;
}

export interface GithubPullRequest {
  id: number;
  number: number;
  title: string;
  state: "open" | "closed";
  draft: boolean;
  merged_at: string | null;
  user: GithubUser | null;
  labels: GithubLabel[];
  head: GithubPullRequestRef;
  base: GithubPullRequestRef;
  created_at: string;
  updated_at: string;
}

export interface GithubRelease {
  id: number;
  tag_name: string;
  name: string | null;
  body: string | null;
  draft: boolean;
  prerelease: boolean;
  created_at: string;
  published_at: string | null;
}

export interface WebhookEvent {
  id: string;
  repo_id: string | null;
  github_event: string;
  delivery_id: string | null;
  payload_json: string;
  signature_valid: number;
  matched_workflow_ids: string;
  received_at: string;
}

// --- Workflow domain model (mirrors backend/src/workflow/model.rs) ---

export interface WorkflowInput {
  description?: string | null;
  required: boolean;
  default?: string | null;
}

export interface PushTrigger {
  branches: string[];
  tags: string[];
  paths: string[];
}

export type PrEventType = "opened" | "synchronize" | "reopened" | "closed" | "labeled" | "unlabeled" | "ready_for_review";

export interface PullRequestTrigger {
  types: PrEventType[];
  branches: string[];
}

export type ReleaseEventType = "published" | "created" | "edited" | "deleted" | "prereleased" | "released";

export interface ReleaseTrigger {
  types: ReleaseEventType[];
}

export type IssuesEventType =
  | "opened"
  | "edited"
  | "closed"
  | "reopened"
  | "labeled"
  | "unlabeled"
  | "assigned"
  | "unassigned";

export interface IssuesTrigger {
  types: IssuesEventType[];
}

export interface ManualTrigger {
  inputs?: Record<string, WorkflowInput> | null;
}

export interface CronTrigger {
  cron: string;
}

export interface TriggerConfig {
  push?: PushTrigger | null;
  pull_request?: PullRequestTrigger | null;
  release?: ReleaseTrigger | null;
  issues?: IssuesTrigger | null;
  workflow_dispatch?: ManualTrigger | null;
  schedule?: CronTrigger[] | null;
}

export interface ContainerSpec {
  image: string;
  env?: Record<string, string> | null;
  volumes: string[];
}

export interface ArtifactSpec {
  name: string;
  path: string;
}

export interface Step {
  name?: string | null;
  id?: string | null;
  run?: string | null;
  uses?: string | null;
  with?: Record<string, unknown> | null;
  env?: Record<string, string> | null;
  if?: string | null;
  "continue-on-error": boolean;
}

export interface Job {
  name?: string | null;
  runs_on: string;
  /** Runs `run:` steps in this container via Docker when set; runs natively via the Bucket
   * sandbox instead when unset (matches real GitHub Actions: no `container:` key means the job
   * runs directly on the runner). */
  container?: ContainerSpec | null;
  needs: string[];
  if?: string | null;
  strategy?: { matrix?: Record<string, string[]> | null } | null;
  steps: Step[];
  artifacts: ArtifactSpec[];
  download_artifacts: string[];
}

export interface WorkflowModel {
  name: string;
  on: TriggerConfig;
  env?: Record<string, string> | null;
  jobs: Record<string, Job>;
}
