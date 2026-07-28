use anyhow::Result;

use crate::app::AppState;
use crate::db::queries::runs as run_queries;
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

pub async fn report_cancelled(state: &AppState, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    let client = client::shared(state).await.map_err(|e| anyhow::anyhow!(e))?;
    crate::github::status::mark_cancelled(&client, owner, repo, sha, target_url).await
}

/// Logs and persists a GitHub status reporting failure onto the run itself (see
/// `WorkflowRun::github_report_error`), so it's visible on the run detail page instead of only
/// discoverable from the server's own console output. Best-effort: if even the persist fails,
/// that's logged too and swallowed, since the run's real outcome was already decided independent
/// of any of this.
pub async fn record_report_failure(state: &AppState, run_id: &str, action: &str, error: &anyhow::Error) {
    tracing::warn!(error = format!("{error:#}"), run_id, action, "failed to report a GitHub status/check update");
    let message = format!("{action}: {error:#}");
    if let Err(e) = run_queries::set_github_report_error(&state.db, run_id, &message).await {
        tracing::warn!(error = %e, run_id, "failed to persist the GitHub status-reporting failure onto the run itself");
    }
}
