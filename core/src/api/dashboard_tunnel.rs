use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::ApprovedUser;
use crate::db::models::DashboardTunnelRequest;
use crate::db::queries::dashboard_tunnel_requests as requests_queries;
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

#[derive(Deserialize)]
pub struct ListRequestsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

/// Audit trail of requests that arrived over the dashboard tunnel specifically (never LAN/local
/// traffic), so an operator can review who/what has been hitting their remote-access tunnel.
pub async fn list_requests(
    State(state): State<AppState>,
    Query(q): Query<ListRequestsQuery>,
    _user: ApprovedUser,
) -> AppResult<Json<Vec<DashboardTunnelRequest>>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    Ok(Json(requests_queries::list_recent(&state.db, limit, offset).await?))
}
