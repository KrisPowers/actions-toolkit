use anyhow::{Context, Result};

use crate::app::AppState;
use crate::db::models::Repo;
use crate::db::queries::{repos as repo_queries, webhook_events as event_queries, workflows as workflow_queries};
use crate::workflow::{trigger_match, yaml};

/// Polling fallback for repos GitHub can't reach with a real webhook (see the webhook-reachability
/// status surfaced elsewhere in the UI): checks the repo's most recent release and, if it's one
/// this instance hasn't already reacted to, synthesizes the same event shape a real `release`
/// webhook delivery would have and runs it through the exact same trigger-matching and dispatch
/// path `webhooks::receive` uses. Returns `true` if a new release was found and dispatched.
pub async fn sync_repo_releases(state: &AppState, repo: &Repo) -> Result<bool> {
    let client = crate::github::client::shared(state).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let releases =
        atk_github::releases::list_releases(&client, &repo.owner, &repo.name).await.context("failed to list releases")?;

    let Some(latest) = releases.into_iter().find(|r| !r.draft) else {
        return Ok(false);
    };
    let latest_id = latest.id.0 as i64;

    if repo.last_synced_release_id == Some(latest_id) {
        return Ok(false);
    }

    let release_json = serde_json::to_value(&latest).context("failed to serialize release")?;
    let payload = serde_json::json!({ "action": "published", "release": release_json });
    let payload_json = payload.to_string();
    let delivery_id = format!("poll-release-{latest_id}");

    let mut matched_ids = Vec::new();
    let workflows = workflow_queries::list_enabled_for_repo(&state.db, &repo.id).await?;
    for workflow_row in workflows {
        let Ok(model) = yaml::parse(&workflow_row.yaml_source) else { continue };
        let Some(matched) = trigger_match::matches(&model, "release", &payload) else { continue };

        matched_ids.push(workflow_row.id.clone());

        if let Err(e) = crate::runner::dispatch::spawn_run(
            state,
            &workflow_row,
            repo,
            "release",
            Some(&payload_json),
            matched.ref_name.as_deref(),
            matched.commit_sha.as_deref(),
        )
        .await
        {
            tracing::error!(error = %e, workflow_id = %workflow_row.id, "failed to spawn run for polled release");
        }
    }

    let matched_json = serde_json::to_string(&matched_ids).unwrap_or_else(|_| "[]".to_string());
    let _ = event_queries::record(&state.db, Some(&repo.id), "release", Some(&delivery_id), &payload_json, true, &matched_json).await;

    repo_queries::set_last_synced_release_id(&state.db, &repo.id, latest_id).await?;

    Ok(true)
}

/// Runs `sync_repo_releases` for every repo without a working webhook, on a fixed interval, for
/// as long as the process runs. Started once at startup, alongside the Bucket TTL reaper.
pub async fn run_periodic_sync(state: AppState, interval: std::time::Duration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let repos = match repo_queries::list_without_webhook(&state.db).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "polling fallback: failed to list repos without a webhook");
                continue;
            }
        };
        for repo in repos {
            match sync_repo_releases(&state, &repo).await {
                Ok(true) => tracing::info!(repo_id = %repo.id, "polling fallback dispatched a new release"),
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, repo_id = %repo.id, "polling fallback sync failed"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppStateInner;
    use crate::auth::jwt::JwtCodec;
    use crate::config::AppConfig;
    use crate::crypto::EncryptionKey;
    use crate::db::queries::users as user_queries;
    use crate::runner::log_stream::LogHub;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_state(mock_server: &MockServer) -> (AppState, String) {
        let test_id = uuid::Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-poll-sync-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let db = crate::db::connect(&data_dir.join("db.sqlite")).await.unwrap();
        let enc = EncryptionKey::load_or_generate(None, &data_dir.join("secrets")).unwrap();
        let config = AppConfig {
            data_dir,
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: crate::github::oauth::GITHUB_TOKEN_URL.to_string(),
            github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
        };
        let user = user_queries::create(&db, "tester", "hash", "admin").await.unwrap();

        let state = AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt: JwtCodec::new("test-secret"),
            enc,
            docker: None,
            bucket_capability_ok: true,
            bucket_capability_reason: None,
            log_hub: Arc::new(LogHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
            token_refresh_lock: tokio::sync::Mutex::new(()),
        }));

        let github_client = octocrab::Octocrab::builder().base_uri(mock_server.uri()).unwrap().personal_token("test-token".to_string()).build().unwrap();
        *state.github_client.write().await = Some(github_client);

        (state, user.id)
    }

    fn mock_release(id: u64, tag_name: &str, draft: bool) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "node_id": format!("R_{id}"),
            "tag_name": tag_name,
            "name": tag_name,
            "draft": draft,
            "prerelease": false,
            "target_commitish": "main",
            "assets": [],
            "url": "https://api.github.com/repos/octocat/hello-world/releases/1",
            "html_url": "https://github.com/octocat/hello-world/releases/tag/v1",
            "assets_url": "https://api.github.com/repos/octocat/hello-world/releases/1/assets",
            "upload_url": "https://uploads.github.com/repos/octocat/hello-world/releases/1/assets",
        })
    }

    /// Rule-proving test: a repo's first sync against a real new release must dispatch a matching
    /// `on: release` workflow and remember the release id, so it doesn't dispatch it again.
    #[tokio::test]
    async fn sync_dispatches_a_new_release_once_and_remembers_it() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello-world/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![mock_release(555, "v1.0.0", false)]))
            .mount(&mock_server)
            .await;

        let (state, user_id) = test_state(&mock_server).await;
        let repo = crate::db::queries::repos::create(&state.db, "octocat", "hello-world", "main", b"s", b"n", &user_id)
            .await
            .unwrap();
        workflow_queries::create(
            &state.db,
            &repo.id,
            "on-release",
            None,
            "release.yml",
            "on:\n  release:\n    types: [published]\njobs:\n  build:\n    runs_on: self-hosted\n    steps: []\n",
            "{}",
        )
        .await
        .unwrap();

        let dispatched = sync_repo_releases(&state, &repo).await.unwrap();
        assert!(dispatched, "expected a brand new release to be reported as dispatched");

        let updated_repo = crate::db::queries::repos::find_by_id(&state.db, &repo.id).await.unwrap().unwrap();
        assert_eq!(updated_repo.last_synced_release_id, Some(555));

        // Second sync against the exact same latest release must be a no-op.
        let dispatched_again = sync_repo_releases(&state, &updated_repo).await.unwrap();
        assert!(!dispatched_again, "must not re-dispatch a release it already synced");
    }

    /// Rule-proving test: a draft release must never be synced, matching real GitHub webhook
    /// behavior (drafts don't fire a `published` event either).
    #[tokio::test]
    async fn sync_ignores_draft_releases() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/octocat/hello-world/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(vec![mock_release(999, "v2.0.0-draft", true)]))
            .mount(&mock_server)
            .await;

        let (state, user_id) = test_state(&mock_server).await;
        let repo = crate::db::queries::repos::create(&state.db, "octocat", "hello-world", "main", b"s", b"n", &user_id)
            .await
            .unwrap();

        let dispatched = sync_repo_releases(&state, &repo).await.unwrap();
        assert!(!dispatched, "a draft-only release list must never be synced");
    }
}
