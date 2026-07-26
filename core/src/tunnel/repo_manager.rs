use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::app::AppState;
use crate::db::queries::repo_tunnels as repo_tunnels_queries;
use crate::db::queries::repos as repo_queries;
use crate::github::{client, hooks};

use super::repo_listener::{self, ListenerHandle};
use super::{TunnelProcess, TunnelProvider, TunnelState};

struct RepoTunnelEntry {
    process: Arc<TunnelProcess>,
    listener: ListenerHandle,
}

/// Tracks every repo's webhook tunnel independently, keyed by repo_id. Each entry owns its own
/// child process (via its `TunnelProcess`) and its own loopback listener (`repo_listener::spawn`)
/// serving only that repo's webhook route -- starting, stopping, or tearing down one repo's entry
/// never touches any other repo's, and no repo's tunnel is ever shared with another's.
#[derive(Default)]
pub struct RepoTunnelManager {
    entries: RwLock<HashMap<String, RepoTunnelEntry>>,
}

impl RepoTunnelManager {
    pub fn new() -> Self {
        Self { entries: RwLock::new(HashMap::new()) }
    }

    pub async fn status(&self, repo_id: &str) -> TunnelState {
        match self.entries.read().await.get(repo_id) {
            Some(entry) => entry.process.status().await,
            None => TunnelState::Idle,
        }
    }

    /// Starts (or, if already running under the same provider, no-ops) repo_id's tunnel: ensures
    /// a loopback listener serving only that repo's webhook route exists, then spawns the tunnel
    /// process pointed at it. Persists the choice so a restart of the whole instance can bring it
    /// back automatically (see `repo_tunnels::list_enabled` and the boot-time loop in `main.rs`).
    pub async fn start(&self, state: &AppState, repo_id: &str, provider: TunnelProvider) -> anyhow::Result<()> {
        let process = {
            let mut entries = self.entries.write().await;
            if let Some(entry) = entries.get(repo_id) {
                entry.process.clone()
            } else {
                let listener = repo_listener::spawn(state.clone(), repo_id.to_string()).await?;
                let process = Arc::new(TunnelProcess::new());
                entries.insert(repo_id.to_string(), RepoTunnelEntry { process: process.clone(), listener });
                process
            }
        };
        let local_port = self.entries.read().await.get(repo_id).expect("just inserted above").listener.local_port;

        super::start(process.clone(), provider, local_port).await;
        repo_tunnels_queries::upsert(&state.db, repo_id, provider.as_str(), true, Some(local_port as i64), None).await?;
        tokio::spawn(reconcile_webhook_on_url_change(state.clone(), repo_id.to_string(), process));
        Ok(())
    }

    /// Stops repo_id's tunnel process but leaves its loopback listener bound, so a subsequent
    /// `start` for the same repo can reuse the same local port instead of allocating a new one.
    pub async fn stop(&self, state: &AppState, repo_id: &str) -> anyhow::Result<()> {
        if let Some(entry) = self.entries.read().await.get(repo_id) {
            super::stop(&entry.process).await;
        }
        repo_tunnels_queries::set_enabled(&state.db, repo_id, false).await?;
        Ok(())
    }

    /// Tears a repo's tunnel down completely: stops the process AND frees its loopback listener's
    /// port. Called when a repo is disconnected, so no orphaned process/listener is left bound to
    /// a now-deleted repo_id.
    pub async fn teardown(&self, repo_id: &str) {
        if let Some(mut entry) = self.entries.write().await.remove(repo_id) {
            super::stop(&entry.process).await;
            entry.listener.shutdown();
        }
    }
}

/// Waits for `process` to report a tunnel URL, then, if it differs from the URL this repo's
/// webhook was last pointed at, re-points GitHub's webhook at it and persists the new URL.
///
/// Quick tunnels (Cloudflare's `trycloudflare.com`) hand out a brand new random hostname every
/// time the process restarts -- app restart, network blip, `cloudflared` reconnecting -- and
/// nothing else in this codebase re-syncs that with GitHub, so without this a webhook silently
/// goes dead (GitHub keeps trying the old, now-unreachable hostname) until an operator notices
/// runs have stopped triggering and manually recreates it. Gives up quietly after ~30s if the
/// tunnel never comes up; `TunnelState::Failed` (surfaced in the UI) already covers that case.
async fn reconcile_webhook_on_url_change(state: AppState, repo_id: String, process: Arc<TunnelProcess>) {
    let url = 'wait: {
        for _ in 0..120 {
            match process.status().await {
                TunnelState::Running { url } => break 'wait url,
                TunnelState::Failed { .. } | TunnelState::Idle => return,
                TunnelState::Starting => tokio::time::sleep(Duration::from_millis(250)).await,
            }
        }
        return;
    };

    let previous_url = repo_tunnels_queries::get(&state.db, &repo_id).await.ok().flatten().and_then(|t| t.last_url);
    if previous_url.as_deref() == Some(url.as_str()) {
        return;
    }

    if let Err(e) = repo_tunnels_queries::set_last_url(&state.db, &repo_id, &url).await {
        tracing::warn!(error = %e, repo_id, "failed to persist the tunnel's discovered URL");
    }

    let Ok(Some(repo)) = repo_queries::find_by_id(&state.db, &repo_id).await else { return };
    // A repo that's never had a GitHub webhook (still mid-connect, or connect failed before the
    // hook was created) has nothing to re-point yet -- the normal connect flow will create it.
    if repo.github_hook_id.is_none() {
        return;
    }
    let Ok(github_client) = client::shared(&state).await else {
        tracing::warn!(repo_id, "tunnel URL changed but no working GitHub connection to re-point the webhook");
        return;
    };
    let Ok(webhook_secret) = state.enc.decrypt_str(&repo.webhook_secret_encrypted, &repo.webhook_secret_nonce) else {
        tracing::warn!(repo_id, "failed to decrypt the webhook secret while re-pointing the webhook");
        return;
    };

    if let Some(hook_id) = repo.github_hook_id {
        if let Err(e) = hooks::delete_webhook(&github_client, &repo.owner, &repo.name, hook_id as u64).await {
            tracing::warn!(error = %e, repo_id, "failed to delete the old GitHub webhook before re-pointing it");
        }
    }

    let payload_url = format!("{url}/webhooks/github/{repo_id}");
    match hooks::create_webhook(&github_client, &repo.owner, &repo.name, &payload_url, &webhook_secret).await {
        Ok(hook_id) => match repo_queries::set_github_hook_id(&state.db, &repo_id, hook_id as i64).await {
            Ok(()) => tracing::info!(repo_id, url, "re-pointed the GitHub webhook after the tunnel's URL changed"),
            Err(e) => tracing::warn!(error = %e, repo_id, "failed to persist the re-pointed webhook's id"),
        },
        Err(e) => tracing::warn!(error = %e, repo_id, "failed to re-point the GitHub webhook at the new tunnel URL"),
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
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_state(mock_server: &MockServer) -> AppState {
        let test_id = uuid::Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-repo-manager-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let db = crate::db::connect(&data_dir.join("db.sqlite")).await.unwrap();
        let enc = EncryptionKey::load_or_generate(None, &data_dir.join("secrets")).unwrap();
        let config = AppConfig {
            data_dir,
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: crate::github::oauth::GITHUB_TOKEN_URL.to_string(),
            github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
        };
        user_queries::upsert_from_github(&db, 1, "tester", None, None, "admin", "approved").await.unwrap();

        let state = AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt: JwtCodec::new("test-secret"),
            enc,
            docker: None,
            bucket_capability_ok: true,
            bucket_capability_reason: None,
            log_hub: Arc::new(LogHub::new()),
            stats_hub: Arc::new(crate::runner::stats_hub::StatsHub::new()),
            activity_hub: Arc::new(crate::runner::activity_hub::ActivityHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
            device_flow_result: RwLock::new(None),
            login_flows: RwLock::new(std::collections::HashMap::new()),
            login_rate_limiter: atk_auth::rate_limit::RateLimiter::new(
                crate::auth::login_flow::LOGIN_RATE_LIMIT_MAX_ATTEMPTS,
                crate::auth::login_flow::LOGIN_RATE_LIMIT_WINDOW,
            ),
            token_refresh_lock: tokio::sync::Mutex::new(()),
            repo_tunnels: Arc::new(RepoTunnelManager::new()),
            dashboard_tunnel: Arc::new(crate::tunnel::dashboard_manager::DashboardTunnelManager::new()),
        }));

        let github_client = octocrab::Octocrab::builder().base_uri(mock_server.uri()).unwrap().personal_token("test-token".to_string()).build().unwrap();
        *state.github_client.write().await = Some(github_client);

        state
    }

    /// Sets up a repo already connected with a live GitHub hook (`existing_hook_id`), plus a
    /// `repo_tunnels` row recording `previous_url` as the last URL its tunnel came up on.
    async fn seed_connected_repo(state: &AppState, previous_url: Option<&str>, existing_hook_id: i64) -> crate::db::models::Repo {
        let (secret_encrypted, secret_nonce) = state.enc.encrypt_str("s3cr3t").unwrap();
        let repo = repo_queries::create(&state.db, "octocat", "hello-world", "main", &secret_encrypted, &secret_nonce, "user-1").await.unwrap();
        repo_queries::set_github_hook_id(&state.db, &repo.id, existing_hook_id).await.unwrap();
        repo_tunnels_queries::upsert(&state.db, &repo.id, "cloudflare", true, Some(4242), previous_url).await.unwrap();
        repo_queries::find_by_id(&state.db, &repo.id).await.unwrap().unwrap()
    }

    /// Rule-proving test: this is the whole point of the reconciliation task -- a tunnel that
    /// comes up on a URL different from the one GitHub's webhook currently points to (the
    /// Cloudflare quick-tunnel restart scenario from issue #129) must get GitHub re-pointed at the
    /// new URL automatically, with no operator action.
    #[tokio::test]
    async fn repoints_the_webhook_when_the_tunnel_url_changed() {
        let mock_server = MockServer::start().await;
        Mock::given(method("DELETE")).and(path("/repos/octocat/hello-world/hooks/111")).respond_with(ResponseTemplate::new(204)).mount(&mock_server).await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/hooks"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": 999 })))
            .mount(&mock_server)
            .await;

        let state = test_state(&mock_server).await;
        let repo = seed_connected_repo(&state, Some("https://old-random-words.trycloudflare.com"), 111).await;

        let process = Arc::new(TunnelProcess::new());
        *process.state.write().await = TunnelState::Running { url: "https://new-random-words.trycloudflare.com".to_string() };

        reconcile_webhook_on_url_change(state.clone(), repo.id.clone(), process).await;

        let updated = repo_queries::find_by_id(&state.db, &repo.id).await.unwrap().unwrap();
        assert_eq!(updated.github_hook_id, Some(999), "must store the freshly created hook's id, not the stale one");

        let tunnel = repo_tunnels_queries::get(&state.db, &repo.id).await.unwrap().unwrap();
        assert_eq!(tunnel.last_url.as_deref(), Some("https://new-random-words.trycloudflare.com"));
    }

    /// Rule-proving test: a tunnel reporting the SAME URL it already had (e.g. Tailscale, whose
    /// hostname is stable across restarts) must not touch GitHub at all -- the mock server has no
    /// routes mounted, so any HTTP call here would fail the request and the assertion below.
    #[tokio::test]
    async fn does_nothing_when_the_tunnel_url_is_unchanged() {
        let mock_server = MockServer::start().await;

        let state = test_state(&mock_server).await;
        let repo = seed_connected_repo(&state, Some("https://stable-host.ts.net"), 111).await;

        let process = Arc::new(TunnelProcess::new());
        *process.state.write().await = TunnelState::Running { url: "https://stable-host.ts.net".to_string() };

        reconcile_webhook_on_url_change(state.clone(), repo.id.clone(), process).await;

        let updated = repo_queries::find_by_id(&state.db, &repo.id).await.unwrap().unwrap();
        assert_eq!(updated.github_hook_id, Some(111), "the untouched hook id proves no GitHub call was made");
    }
}
