pub mod analytics;
pub mod artifacts;
pub mod github_account;
pub mod github_oauth;
pub mod github_proxy;
pub mod repos;
pub mod runs;
pub mod settings;
pub mod static_files;
pub mod webhooks;
pub mod workflows;

use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::app::AppState;
use crate::auth::handlers as auth_handlers;

/// Derives `scheme://host` from the incoming request's own `Host` header (and
/// `X-Forwarded-Proto`, set by a tunnel/proxy terminating TLS in front of a plain-HTTP backend),
/// since actions-toolkit runs on whatever host:port the operator chose and has no fixed public
/// URL of its own to fall back on. Used to build URLs GitHub needs to call back into this
/// instance: the OAuth callback and a repo's webhook payload URL.
pub(crate) fn request_origin(headers: &axum::http::HeaderMap) -> String {
    let host = headers.get(axum::http::header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("localhost:7890");
    let scheme = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).unwrap_or("http");
    format!("{scheme}://{host}")
}

pub fn router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/auth/status", get(auth_handlers::status))
        .route("/auth/setup", post(auth_handlers::setup))
        .route("/auth/login", post(auth_handlers::login))
        .route("/auth/logout", post(auth_handlers::logout))
        .route("/auth/me", get(auth_handlers::me))
        .route("/auth/github/authorize", get(github_oauth::authorize))
        .route("/auth/github/callback", get(github_oauth::callback))
        .route("/users", get(auth_handlers::list_users).post(auth_handlers::create_user))
        .route("/users/{id}", delete(auth_handlers::delete_user))
        .route(
            "/github/token",
            get(github_account::status).post(github_account::set_token).delete(github_account::delete_token),
        )
        .route("/github/accessible-repos", get(github_account::accessible_repos))
        .route("/settings", get(settings::get).patch(settings::update))
        .route("/settings/runtime-status", get(settings::runtime_status))
        .route("/repos", get(repos::list).post(repos::create))
        .route("/repos/{id}", get(repos::get).delete(repos::delete))
        .route("/repos/{id}/test-connection", post(repos::test_connection))
        .route("/repos/{id}/webhook-events", get(repos::webhook_events))
        .route("/repos/{repo_id}/workflows", get(workflows::list_for_repo).post(workflows::create))
        .route("/repos/{repo_id}/workflows/export", get(workflows::export_repo))
        .route("/workflows/{id}", get(workflows::get).patch(workflows::update).delete(workflows::delete))
        .route("/workflows/{id}/enabled", patch(workflows::set_enabled))
        .route("/workflows/{id}/dispatch", post(workflows::dispatch))
        .route("/workflows/{id}/export", get(workflows::export))
        .route("/workflows/validate", post(workflows::validate_workflow))
        .route("/repos/{repo_id}/runs", get(runs::list_for_repo))
        .route("/runs/{id}", get(runs::get))
        .route("/runs/{id}/cancel", post(runs::cancel))
        .route("/runs/{id}/rerun", post(runs::rerun))
        .route("/runs/{id}/logs", get(runs::logs))
        .route("/runs/{id}/logs/ws", get(crate::ws::run_logs_ws))
        .route("/runs/{id}/artifacts", get(artifacts::list_for_run))
        .route("/repos/{repo_id}/artifacts", get(artifacts::list_for_repo))
        .route("/artifacts/{id}/download", get(artifacts::download))
        .route("/repos/{repo_id}/issues", get(github_proxy::list_issues))
        .route("/repos/{repo_id}/issues/{number}", get(github_proxy::get_issue).patch(github_proxy::update_issue))
        .route("/repos/{repo_id}/issues/{number}/comments", post(github_proxy::add_comment))
        .route("/repos/{repo_id}/pulls", get(github_proxy::list_pull_requests))
        .route("/repos/{repo_id}/pulls/{number}", get(github_proxy::get_pull_request))
        .route("/repos/{repo_id}/pulls/{number}/comments", post(github_proxy::add_comment))
        .route(
            "/repos/{repo_id}/github-workflows",
            get(github_proxy::list_github_workflows),
        )
        .route(
            "/repos/{repo_id}/github-workflows/import",
            post(github_proxy::import_github_workflow),
        )
        .route("/repos/{repo_id}/releases", get(github_proxy::list_releases).post(github_proxy::create_release))
        .route(
            "/repos/{repo_id}/releases/{release_id}",
            get(github_proxy::get_release).patch(github_proxy::update_release),
        )
        .route("/repos/{repo_id}/analytics/summary", get(analytics::summary))
        .route("/repos/{repo_id}/analytics/duration-trend", get(analytics::duration_trend))
        .route("/repos/{repo_id}/analytics/status-breakdown", get(analytics::status_breakdown));

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .nest("/api", api_routes)
        .route("/webhooks/github/{repo_id}", post(webhooks::receive))
        .fallback(static_files::spa_fallback)
        .route("/", get(static_files::spa_root))
        .layer(TraceLayer::new_for_http())
        // Permissive CORS: this server is meant to be reached only from the local network by
        // the operator's own frontend build; tighten this if ever exposed beyond that.
        .layer(CorsLayer::permissive())
        .with_state(state)
}
