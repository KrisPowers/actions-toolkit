use axum::extract::State;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::Settings;
use crate::db::queries::settings::{self as settings_queries, SettingsPatch};
use crate::error::AppResult;
use crate::tunnel::CloudflareTunnelState;

pub async fn get(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Settings>> {
    Ok(Json(settings_queries::get(&state.db).await?))
}

#[derive(Serialize)]
pub struct RuntimeStatus {
    pub docker_available: bool,
    pub bucket_available: bool,
    pub bucket_unavailable_reason: Option<String>,
}

/// Re-checks Docker live rather than trusting the value captured at startup, so starting (or
/// stopping) the Docker daemon while the app is already running is reflected without a restart.
/// Bucket's capability isn't re-probed here: unlike Docker it isn't a service the user starts or
/// stops, and the real probe actually spins up a throwaway sandbox, too heavy to run on every
/// poll from the UI.
pub async fn runtime_status(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<RuntimeStatus>> {
    let settings = settings_queries::get(&state.db).await?;
    let docker_available = match crate::runner::docker::connect(settings.docker_host.as_deref()) {
        Ok(client) => crate::runner::docker::ping(&client).await.is_ok(),
        Err(_) => false,
    };
    Ok(Json(RuntimeStatus {
        docker_available,
        bucket_available: state.bucket_capability_ok,
        bucket_unavailable_reason: (!state.bucket_capability_ok).then(|| state.bucket_capability_reason.clone()).flatten(),
    }))
}

/// `port` is deliberately not accepted here: changing it always requires a restart to take
/// effect (the listener is already bound by the time the UI can reach the server), so it's
/// changed via `actions-toolkit start --port <n>` instead, which persists it for next time.
#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub bind_addr: Option<String>,
    pub docker_host: Option<String>,
    pub max_concurrent_jobs: Option<usize>,
    pub bucket_default_ttl_seconds: Option<i64>,
    /// `0` (not a meaningful CPU/memory limit) is treated as "clear the limit back to
    /// unbounded"; omitted entirely leaves it untouched; any other value sets it. Simpler than a
    /// nested-Option "nullable-but-optional" field, at the cost of not being able to set an
    /// actual `0` limit, which would mean "no CPU/memory at all" and isn't a real value anyone
    /// wants regardless.
    pub bucket_cpu_limit_millis: Option<i64>,
    pub bucket_memory_limit_mb: Option<i64>,
    pub bucket_host_mounts_json: Option<String>,
    /// An empty string clears the override back to auto-detect from the request; anything else
    /// sets it; the field being absent entirely leaves it untouched.
    pub public_url: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    _user: CurrentUser,
    Json(req): Json<UpdateSettingsRequest>,
) -> AppResult<Json<Settings>> {
    let patch = SettingsPatch {
        port: None,
        bind_addr: req.bind_addr,
        // An empty string means "clear the override and auto-detect again".
        docker_host: req.docker_host.map(|s| { let s = s.trim().to_string(); if s.is_empty() { None } else { Some(s) } }),
        max_concurrent_jobs: req.max_concurrent_jobs,
        bucket_default_ttl_seconds: req.bucket_default_ttl_seconds,
        bucket_cpu_limit_millis: req.bucket_cpu_limit_millis.map(|v| if v == 0 { None } else { Some(v) }),
        bucket_memory_limit_mb: req.bucket_memory_limit_mb.map(|v| if v == 0 { None } else { Some(v) }),
        bucket_host_mounts_json: req.bucket_host_mounts_json,
        public_url: req.public_url.map(|s| { let s = s.trim().to_string(); if s.is_empty() { None } else { Some(s) } }),
    };
    Ok(Json(settings_queries::update(&state.db, patch).await?))
}

#[derive(Serialize)]
pub struct NetworkInfo {
    /// `None` when the outbound lookup fails (no internet, timeout, non-2xx) rather than an
    /// error response, since this is a purely informational display and the rest of the Webhooks
    /// page still needs to render.
    pub public_ip: Option<String>,
    pub port: i64,
    pub configured_public_url: Option<String>,
    /// Literal path template (matches `repos::to_public`'s `webhook_url` construction) so the
    /// frontend can splice in a repo id without duplicating the path format in two languages.
    pub webhook_path_template: String,
}

const IPIFY_URL: &str = "https://api.ipify.org";

/// Fetches the plain-text IP body from an ipify-shaped echo endpoint, tolerating any failure (no
/// outbound internet, timeout, non-2xx, empty body) as `None` rather than an error, since this is
/// a purely informational lookup. Takes the URL as a parameter so tests can point it at a mock
/// server (or an address that fails fast) instead of reaching the real internet.
async fn fetch_public_ip(url: &str) -> Option<String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build().ok()?;
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().await.ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Best-effort public-IP lookup for the "manual port forward" webhook quick action.
pub async fn network_info(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<NetworkInfo>> {
    let settings = settings_queries::get(&state.db).await?;
    let public_ip = fetch_public_ip(IPIFY_URL).await;

    Ok(Json(NetworkInfo {
        public_ip,
        port: settings.port,
        configured_public_url: settings.public_url,
        webhook_path_template: "/webhooks/github/{repo_id}".to_string(),
    }))
}

/// Starts (or reports the status of) the instance-wide Cloudflare Quick Tunnel, so the operator
/// never has to run `cloudflared` in a terminal themselves. Fire-and-poll: this returns
/// immediately with whatever the state is right after kicking off the spawn (usually `Starting`);
/// the frontend polls `GET /settings/cloudflare-tunnel` until it flips to `Running` or `Failed`.
pub async fn start_cloudflare_tunnel(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<CloudflareTunnelState>> {
    let settings = settings_queries::get(&state.db).await?;
    crate::tunnel::start(state.cloudflare_tunnel.clone(), settings.port as u16).await;
    Ok(Json(state.cloudflare_tunnel.status().await))
}

pub async fn cloudflare_tunnel_status(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<CloudflareTunnelState>> {
    Ok(Json(state.cloudflare_tunnel.status().await))
}

pub async fn stop_cloudflare_tunnel(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<CloudflareTunnelState>> {
    crate::tunnel::stop(&state.cloudflare_tunnel).await;
    Ok(Json(state.cloudflare_tunnel.status().await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_public_ip_returns_the_trimmed_body_on_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200).set_body_string("203.0.113.42\n")).mount(&mock_server).await;

        assert_eq!(fetch_public_ip(&mock_server.uri()).await, Some("203.0.113.42".to_string()));
    }

    #[tokio::test]
    async fn fetch_public_ip_is_none_when_the_lookup_fails() {
        // Nothing is listening here, so the connection itself fails fast rather than timing out.
        assert_eq!(fetch_public_ip("http://127.0.0.1:1").await, None);
    }

    #[tokio::test]
    async fn fetch_public_ip_is_none_on_a_non_success_status() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(503)).mount(&mock_server).await;

        assert_eq!(fetch_public_ip(&mock_server.uri()).await, None);
    }
}
