use axum::extract::State;
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::ApprovedUser;
use crate::error::{AppError, AppResult};
use crate::tunnel::{TunnelProvider, TunnelState};

#[derive(Deserialize)]
pub struct StartDashboardTunnelRequest {
    pub provider: TunnelProvider,
}

/// The dashboard/API remote-access tunnel is a singleton, separate from every repo's webhook
/// tunnel (`api::repo_tunnels`): reachable by any `ApprovedUser`, but only through the hardened
/// listener built in `tunnel::dashboard_manager`, never sharing a process or port with any
/// webhook tunnel.
pub async fn status(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<Json<TunnelState>> {
    Ok(Json(state.dashboard_tunnel.status().await))
}

pub async fn start(
    State(state): State<AppState>,
    _user: ApprovedUser,
    Json(req): Json<StartDashboardTunnelRequest>,
) -> AppResult<Json<TunnelState>> {
    state.dashboard_tunnel.start(&state, req.provider).await.map_err(AppError::Internal)?;
    Ok(Json(state.dashboard_tunnel.status().await))
}

pub async fn stop(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<Json<TunnelState>> {
    state.dashboard_tunnel.stop(&state).await.map_err(AppError::Internal)?;
    Ok(Json(state.dashboard_tunnel.status().await))
}
