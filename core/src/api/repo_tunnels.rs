use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::ApprovedUser;
use crate::error::{AppError, AppResult};
use crate::tunnel::TunnelProvider;
use crate::tunnel::TunnelState;

#[derive(Deserialize)]
pub struct StartRepoTunnelRequest {
    pub provider: TunnelProvider,
}

/// Every repo's webhook tunnel is tracked independently in `AppState.repo_tunnels`: starting,
/// stopping, or checking one repo's tunnel here never touches another repo's, and no repo's
/// tunnel is ever shared with another's or with the dashboard tunnel.
pub async fn status(State(state): State<AppState>, Path(repo_id): Path<String>, _user: ApprovedUser) -> AppResult<Json<TunnelState>> {
    Ok(Json(state.repo_tunnels.status(&repo_id).await))
}

pub async fn start(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: ApprovedUser,
    Json(req): Json<StartRepoTunnelRequest>,
) -> AppResult<Json<TunnelState>> {
    crate::db::queries::repos::find_by_id(&state.db, &repo_id).await?.ok_or(AppError::NotFound)?;
    state.repo_tunnels.start(&state, &repo_id, req.provider).await.map_err(AppError::Internal)?;
    Ok(Json(state.repo_tunnels.status(&repo_id).await))
}
