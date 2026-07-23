use std::sync::Arc;

use anyhow::{Context, Result};

use crate::app::AppState;
use crate::db::models::{Repo, WorkflowRun, Workflow as WorkflowRow};
use crate::db::queries::{buckets as bucket_queries, runs as run_queries, shells as shell_queries};
use crate::runner::executor::CheckoutContext;
use crate::runner::shell_run::ShellRunSpec;
use crate::workflow::{model::Workflow, yaml};

/// Create a `workflow_runs` row (+ one `job_runs` row per job), make sure this triggering event
/// has a bucket (reusing one already open for the same webhook delivery so N matched workflows
/// share one RCP server/resource cache/ephemeral key instead of each getting their own), spawn
/// this run's shell subprocess inside it, and spawn a supervisor to report the outcome back to
/// GitHub once the shell exits. Shared by manual dispatch, rerun, and webhook-triggered runs so
/// all three paths stay consistent.
#[allow(clippy::too_many_arguments)]
pub async fn spawn_run(
    state: &AppState,
    workflow_row: &WorkflowRow,
    repo: &Repo,
    trigger_event: &str,
    trigger_payload_json: Option<&str>,
    ref_name: Option<&str>,
    commit_sha: Option<&str>,
    webhook_event_id: Option<&str>,
) -> Result<WorkflowRun> {
    let model: Workflow = yaml::parse(&workflow_row.yaml_source)?;

    let run = run_queries::create_run(
        &state.db,
        &workflow_row.id,
        &repo.id,
        trigger_event,
        trigger_payload_json,
        ref_name,
        commit_sha,
        webhook_event_id,
    )
    .await?;

    let mut job_runs = Vec::new();
    for (job_key, job) in &model.jobs {
        let needs_json = serde_json::to_string(&job.needs).unwrap_or_else(|_| "[]".to_string());
        let job_run = run_queries::create_job_run(&state.db, &run.id, job_key, job.name.as_deref(), &needs_json).await?;
        job_runs.push((job_key.clone(), job_run.id));
    }

    let pat = crate::github::client::decrypted_token(state).await?;
    let checkout = Some(CheckoutContext {
        owner: repo.owner.clone(),
        repo: repo.name.clone(),
        repo_id: repo.id.clone(),
        pat,
        git_ref: ref_name.map(str::to_string).unwrap_or_else(|| format!("refs/heads/{}", repo.default_branch)),
    });

    // Best-effort: a commit-triggered run reports its status back to that commit on GitHub (the
    // same place a real GitHub Actions run would show a check), so pushing to a connected repo
    // shows up there too, not just in this instance's own UI. Skipped for runs with no specific
    // commit (e.g. a manual "Run now" against no particular ref) since there'd be nothing to post
    // it against; failures here are logged, not fatal, since the run itself already succeeded or
    // failed independent of whether GitHub could be told about it.
    if let Some(sha) = commit_sha {
        let target_url = crate::runner::github_status::run_target_url(state, &run.id).await;
        if let Err(e) = crate::runner::github_status::report_pending(state, &repo.owner, &repo.name, sha, target_url).await {
            tracing::warn!(error = format!("{e:#}"), repo = %repo.id, sha, "failed to post the pending GitHub commit status");
        }
    }

    let (bucket_id, rcp_endpoint, auth_token) = ensure_bucket(state, trigger_event, webhook_event_id, &repo.id).await?;

    let max_concurrent_jobs = crate::db::queries::settings::get(&state.db).await.map(|s| s.max_concurrent_jobs.max(1) as usize).unwrap_or(1);

    let shell = shell_queries::create(&state.db, &bucket_id, &run.id, std::env::consts::OS).await?;

    let spec = ShellRunSpec {
        bucket_id: bucket_id.clone(),
        shell_id: shell.id.clone(),
        rcp_endpoint,
        auth_token,
        workflow_run_id: run.id.clone(),
        workflow: model,
        job_runs,
        checkout,
        trigger_event: trigger_event.to_string(),
        max_concurrent_jobs,
        workspaces_dir: state.config.workspaces_dir(),
        buckets_dir: state.config.buckets_dir(),
        artifacts_dir: state.config.artifacts_dir(),
    };

    let spec_path = state.config.data_dir.join(format!("shell-spec-{}.json", shell.id));
    std::fs::write(&spec_path, serde_json::to_vec(&spec).context("failed to serialize shell run spec")?)
        .context("failed to write shell run spec")?;

    let current_exe = std::env::current_exe().context("failed to resolve current executable path")?;
    let child = tokio::process::Command::new(&current_exe)
        .arg("__shell-run")
        .arg(&spec_path)
        .spawn()
        .context("failed to spawn shell subprocess")?;

    if let Some(pid) = child.id() {
        let _ = shell_queries::set_pid(&state.db, &shell.id, pid as i64).await;
    }

    let state_arc = Arc::new(state.clone());
    let run_id = run.id.clone();
    let commit_sha = commit_sha.map(str::to_string);
    let owner = repo.owner.clone();
    let name = repo.name.clone();
    tokio::spawn(crate::runner::scheduler::supervise_shell(state_arc, child, run_id, owner, name, commit_sha));

    Ok(run)
}

/// Reuses the still-open bucket for this webhook delivery if one already exists (so N workflows
/// matched by the same delivery share one bucket), otherwise provisions a fresh one — including a
/// synthetic bucket-per-dispatch for manual/schedule triggers, which have no webhook event to key
/// off of. Starts that bucket's RCP server the first time it's created, never again on reuse.
async fn ensure_bucket(state: &AppState, trigger_kind: &str, webhook_event_id: Option<&str>, repo_id: &str) -> Result<(String, String, String)> {
    if let Some(webhook_event_id) = webhook_event_id {
        if let Some(existing) = bucket_queries::find_open_for_webhook_event(&state.db, webhook_event_id).await? {
            // The auth token itself was never persisted (only its hash), so a reused bucket's
            // plaintext token has to come from wherever it's still held in memory. Phase 1 keeps
            // it simple: re-derive nothing, instead keep a process-wide map from bucket id to its
            // plaintext token alongside the RCP server. See `BUCKET_TOKENS`.
            if let Some(token) = bucket_tokens::get(&existing.id) {
                return Ok((existing.id, existing.rcp_endpoint, token));
            }
        }
    }

    let mut token_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_bytes);
    let auth_token = hex::encode(token_bytes);
    let auth_token_hash = atk_auth::password::hash(&auth_token).context("failed to hash bucket auth token")?;
    let rcp_endpoint = atk_rcp::endpoint_for_bucket(&uuid::Uuid::new_v4().to_string());

    let bucket = bucket_queries::create(&state.db, trigger_kind, webhook_event_id, repo_id, &auth_token_hash, &rcp_endpoint).await?;
    bucket_tokens::set(&bucket.id, &auth_token);

    crate::runner::bucket_server::spawn(
        state.db.clone(),
        state.log_hub.clone(),
        bucket.id.clone(),
        repo_id.to_string(),
        auth_token_hash,
        rcp_endpoint.clone(),
        state.enc.clone(),
    );

    Ok((bucket.id, rcp_endpoint, auth_token))
}

/// Plaintext bucket auth tokens, held only in this control-plane process's memory for as long as
/// the bucket stays open, never persisted (only their hash is, in `buckets.auth_token_hash`).
/// Needed so a second workflow matched by the same webhook delivery can join the first's
/// already-running bucket without the token round-tripping through the database.
mod bucket_tokens {
    use std::sync::LazyLock;

    use dashmap::DashMap;

    static TOKENS: LazyLock<DashMap<String, String>> = LazyLock::new(DashMap::new);

    pub fn set(bucket_id: &str, token: &str) {
        TOKENS.insert(bucket_id.to_string(), token.to_string());
    }

    pub fn get(bucket_id: &str) -> Option<String> {
        TOKENS.get(bucket_id).map(|t| t.clone())
    }
}
