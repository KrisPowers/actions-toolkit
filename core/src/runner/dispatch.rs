use std::sync::Arc;

use anyhow::{Context, Result};

use crate::app::AppState;
use crate::db::models::{Repo, WorkflowRun, Workflow as WorkflowRow};
use crate::db::queries::{agents as agent_queries, buckets as bucket_queries, runs as run_queries, shells as shell_queries};
use crate::runner::executor::CheckoutContext;
use crate::runner::shell_run::ShellRunSpec;
use crate::workflow::{model::Workflow, yaml};

/// Create a `workflow_runs` row (+ one `job_runs` row per job), make sure this triggering event
/// has a bucket (reusing one already open for the same webhook delivery so N matched workflows
/// share one RCP server/resource cache/ephemeral key instead of each getting their own), spawn
/// this run's shell — locally, or scheduled onto a matching remote agent if every job in the run
/// agrees on a `runs_on` this host can't satisfy itself — and spawn a supervisor to report the
/// outcome back to GitHub once it exits. Shared by manual dispatch, rerun, and webhook-triggered
/// runs so all three paths stay consistent.
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
    // Best-effort GitHub Check Run: this is what actually renders the check mark/X/yellow-spinner
    // GitHub shows next to a commit and in a PR's checks list, real GitHub Actions' own UI for
    // exactly this. Kept alongside the plainer commit status above (older, simpler, still what
    // some external tooling and branch-protection setups key off), not a replacement for it.
    let mut check_run_id = None;
    if let Some(sha) = commit_sha {
        let target_url = crate::runner::github_status::run_target_url(state, &run.id).await;
        match crate::runner::github_status::start_check(state, &repo.owner, &repo.name, sha, target_url.clone()).await {
            Ok(id) => check_run_id = Some(id),
            Err(e) => tracing::warn!(error = format!("{e:#}"), repo = %repo.id, sha, "failed to start the GitHub check run"),
        }
        if let Err(e) = crate::runner::github_status::report_pending(state, &repo.owner, &repo.name, sha, target_url).await {
            tracing::warn!(error = format!("{e:#}"), repo = %repo.id, sha, "failed to post the pending GitHub commit status");
        }
    }

    let agent = match resolve_target_os(&model) {
        Some(target) if target != std::env::consts::OS => match select_agent(state, &target).await? {
            Some(agent) => Some(agent),
            None => {
                // Fail fast rather than hang: matches the plan's call for a clear error when a
                // required runs_on can't be satisfied, instead of `runs_on` silently being
                // parsed-but-ignored the way it was before agents existed.
                run_queries::set_run_status(&state.db, &run.id, "failed", true).await?;
                anyhow::bail!("no approved, online agent matches this run's required runs_on ({target}); nothing to schedule it on");
            }
        },
        _ => None,
    };

    let bucket = ensure_bucket(state, trigger_event, webhook_event_id, &repo.id).await?;
    let max_concurrent_jobs = crate::db::queries::settings::get(&state.db).await.map(|s| s.max_concurrent_jobs.max(1) as usize).unwrap_or(1);

    let target_os = agent.as_ref().map(|a| a.os.clone()).unwrap_or_else(|| std::env::consts::OS.to_string());
    let shell_id = uuid::Uuid::new_v4().to_string();

    let spec = ShellRunSpec {
        bucket_id: bucket.id.clone(),
        shell_id: shell_id.clone(),
        rcp_endpoint: if agent.is_some() { bucket.tcp_endpoint.clone() } else { bucket.local_endpoint.clone() },
        auth_token: bucket.auth_token.clone(),
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

    match agent {
        Some(agent) => {
            let spec_json = serde_json::to_string(&spec).context("failed to serialize shell run spec")?;
            let shell = shell_queries::create(&state.db, &shell_id, &bucket.id, &run.id, &target_os, Some(&agent.id), "assigned", Some(&spec_json)).await?;
            tracing::info!(shell_id = %shell.id, agent_id = %agent.id, agent_name = %agent.name, "scheduled shell onto a remote agent");

            let state_arc = Arc::new(state.clone());
            let run_id = run.id.clone();
            let commit_sha = commit_sha.map(str::to_string);
            let owner = repo.owner.clone();
            let name = repo.name.clone();
            tokio::spawn(crate::runner::scheduler::supervise_remote_shell(state_arc, run_id, owner, name, commit_sha, check_run_id));
        }
        None => {
            let shell = shell_queries::create(&state.db, &shell_id, &bucket.id, &run.id, &target_os, None, "running", None).await?;

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
            tokio::spawn(crate::runner::scheduler::supervise_shell(state_arc, child, run_id, owner, name, commit_sha, check_run_id));
        }
    }

    Ok(run)
}

/// Resolves the single `runs_on` target every job in `workflow` agrees on, normalized to an OS
/// name (`linux`/`windows`/`macos`) or a literal custom label. Returns `None` when every job is
/// `self-hosted` (no preference) or when jobs disagree — a workflow with genuinely mixed per-job
/// OS requirements isn't split across multiple shells yet (that needs shell granularity finer
/// than "one per workflow run", a bigger change than this pass makes), so it falls back to
/// running locally rather than failing outright.
fn resolve_target_os(workflow: &Workflow) -> Option<String> {
    let mut target: Option<String> = None;
    for job in workflow.jobs.values() {
        let Some(os) = normalize_runs_on(&job.runs_on) else { continue };
        match &target {
            None => target = Some(os),
            Some(existing) if *existing != os => return None,
            _ => {}
        }
    }
    target
}

fn normalize_runs_on(runs_on: &str) -> Option<String> {
    let lower = runs_on.to_ascii_lowercase();
    if lower == "self-hosted" {
        None
    } else if lower.starts_with("ubuntu") {
        Some("linux".to_string())
    } else if lower.starts_with("windows") {
        Some("windows".to_string())
    } else if lower.starts_with("macos") {
        Some("macos".to_string())
    } else {
        Some(lower)
    }
}

/// Picks the first approved/online agent whose `os` matches `target`, or whose `labels_json`
/// contains it as a literal label (for the non-OS `runs_on` case). No load-balancing across
/// multiple matches yet — every match is currently treated as equally good.
async fn select_agent(state: &AppState, target: &str) -> Result<Option<atk_db::models::Agent>> {
    let candidates = agent_queries::list_available(&state.db).await?;
    Ok(candidates.into_iter().find(|a| {
        a.os == target || serde_json::from_str::<Vec<String>>(&a.labels_json).unwrap_or_default().iter().any(|l| l == target)
    }))
}

struct BucketEndpoints {
    id: String,
    local_endpoint: String,
    tcp_endpoint: String,
    auth_token: String,
}

/// Reuses the still-open bucket for this webhook delivery if one already exists (so N workflows
/// matched by the same delivery share one bucket), otherwise provisions a fresh one — including a
/// synthetic bucket-per-dispatch for manual/schedule triggers, which have no webhook event to key
/// off of. Starts that bucket's RCP server the first time it's created, never again on reuse.
async fn ensure_bucket(state: &AppState, trigger_kind: &str, webhook_event_id: Option<&str>, repo_id: &str) -> Result<BucketEndpoints> {
    if let Some(webhook_event_id) = webhook_event_id {
        if let Some(existing) = bucket_queries::find_open_for_webhook_event(&state.db, webhook_event_id).await? {
            // Neither the auth token nor the TCP address are persisted (only the token's hash
            // is), so a reused bucket's live connection details have to come from wherever
            // they're still held in memory — see `bucket_registry` below.
            if let Some(entry) = bucket_registry::get(&existing.id) {
                return Ok(BucketEndpoints { id: existing.id, local_endpoint: existing.rcp_endpoint, tcp_endpoint: entry.tcp_endpoint, auth_token: entry.auth_token });
            }
        }
    }

    let mut token_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_bytes);
    let auth_token = hex::encode(token_bytes);
    let auth_token_hash = atk_auth::password::hash(&auth_token).context("failed to hash bucket auth token")?;
    let local_endpoint = atk_rcp::endpoint_for_bucket(&uuid::Uuid::new_v4().to_string());

    let bucket = bucket_queries::create(&state.db, trigger_kind, webhook_event_id, repo_id, &auth_token_hash, &local_endpoint).await?;

    let tcp_addr = crate::runner::bucket_server::spawn(
        state.db.clone(),
        state.log_hub.clone(),
        bucket.id.clone(),
        repo_id.to_string(),
        auth_token_hash,
        local_endpoint.clone(),
        state.enc.clone(),
    )
    .await
    .context("failed to start this bucket's RCP server")?;

    // `0.0.0.0:<port>` is what a listener bound to all interfaces reports for itself; a remote
    // agent needs an actually-reachable address, not the wildcard one.
    let public_host = crate::db::queries::settings::get(&state.db)
        .await
        .ok()
        .and_then(|s| s.public_url)
        .and_then(|url| url.split("://").nth(1).map(|s| s.split(['/', ':']).next().unwrap_or("").to_string()))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let tcp_endpoint = format!("{public_host}:{}", tcp_addr.port());

    bucket_registry::set(&bucket.id, &auth_token, &tcp_endpoint);

    Ok(BucketEndpoints { id: bucket.id, local_endpoint, tcp_endpoint, auth_token })
}

/// Live connection details for every bucket this process currently owns, held only in memory for
/// as long as the bucket stays open — the plaintext auth token is never persisted (only its hash
/// is, in `buckets.auth_token_hash`), and the TCP address is only ever meaningful for the process
/// that actually bound the listener. Needed so a second workflow matched by the same webhook
/// delivery can join the first's already-running bucket without either round-tripping through the
/// database.
mod bucket_registry {
    use std::sync::LazyLock;

    use dashmap::DashMap;

    pub struct Entry {
        pub auth_token: String,
        pub tcp_endpoint: String,
    }

    static REGISTRY: LazyLock<DashMap<String, Entry>> = LazyLock::new(DashMap::new);

    pub fn set(bucket_id: &str, auth_token: &str, tcp_endpoint: &str) {
        REGISTRY.insert(bucket_id.to_string(), Entry { auth_token: auth_token.to_string(), tcp_endpoint: tcp_endpoint.to_string() });
    }

    pub fn get(bucket_id: &str) -> Option<Entry> {
        REGISTRY.get(bucket_id).map(|e| Entry { auth_token: e.auth_token.clone(), tcp_endpoint: e.tcp_endpoint.clone() })
    }
}
