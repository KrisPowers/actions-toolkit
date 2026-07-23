use anyhow::Result;

use crate::app::AppState;
use crate::db::queries::settings as settings_queries;
use crate::github::client;

/// Builds the link a GitHub commit status points back at, pointing to this run in this
/// instance's own UI. `None` when no pinned `public_url` is configured (a bare LAN/localhost
/// address wouldn't be a useful link for anyone viewing the status on GitHub).
pub async fn run_target_url(state: &AppState, run_id: &str) -> Option<String> {
    let settings = settings_queries::get(&state.db).await.ok()?;
    let base = settings.public_url.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    Some(format!("{}/runs/{run_id}", base.trim_end_matches('/')))
}

pub async fn report_pending(state: &AppState, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    let client = client::shared(state).await.map_err(|e| anyhow::anyhow!(e))?;
    crate::github::status::mark_pending(&client, owner, repo, sha, target_url).await
}

pub async fn report_success(state: &AppState, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    let client = client::shared(state).await.map_err(|e| anyhow::anyhow!(e))?;
    crate::github::status::mark_success(&client, owner, repo, sha, target_url).await
}

pub async fn report_failure(state: &AppState, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    let client = client::shared(state).await.map_err(|e| anyhow::anyhow!(e))?;
    crate::github::status::mark_failure(&client, owner, repo, sha, target_url).await
}

/// Starts a GitHub check run (the check mark/X/yellow-spinner UI next to a commit, in a PR's
/// checks list, and in branch protection) for a commit-triggered run, returning its id so the
/// caller can mark it completed once the run finishes.
pub async fn start_check(state: &AppState, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<u64> {
    let client = client::shared(state).await.map_err(|e| anyhow::anyhow!(e))?;
    crate::github::checks::start(&client, owner, repo, sha, target_url).await
}

pub async fn complete_check(state: &AppState, owner: &str, repo: &str, check_run_id: u64, succeeded: bool) -> Result<()> {
    let client = client::shared(state).await.map_err(|e| anyhow::anyhow!(e))?;
    crate::github::checks::complete(&client, owner, repo, check_run_id, succeeded).await
}
