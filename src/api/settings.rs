use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::Settings;
use crate::db::queries::settings::{self as settings_queries, SettingsPatch};
use crate::error::AppResult;

pub async fn get(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Settings>> {
    Ok(Json(settings_queries::get(&state.db).await?))
}

/// `port` is deliberately not accepted here: changing it always requires a restart to take
/// effect (the listener is already bound by the time the UI can reach the server), so it's
/// changed via `actions-toolkit start --port <n>` instead, which persists it for next time.
#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub bind_addr: Option<String>,
    pub docker_host: Option<String>,
    pub max_concurrent_jobs: Option<usize>,
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
    };
    Ok(Json(settings_queries::update(&state.db, patch).await?))
}
