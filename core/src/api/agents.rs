//! Operator-facing agent management (join tokens, list/approve/revoke) plus the small REST
//! surface an agent process itself calls (join, heartbeat, poll for assigned shells, fetch a
//! shell's spec, report it started). Agent calls authenticate with a bearer token issued at join
//! time (see the security note on `atk_rcp::tcp` â€” this is not yet backed by mTLS), never a
//! browser session.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::ApprovedUser;
use crate::db::models::Agent;
use crate::db::queries::{agent_join_tokens as join_token_queries, agents as agent_queries, shells as shell_queries};
use crate::error::{AppError, AppResult};

const JOIN_TOKEN_TTL_SECONDS: i64 = 15 * 60;

#[derive(Serialize)]
pub struct JoinTokenResponse {
    pub token: String,
    pub expires_in_seconds: i64,
}

/// Operator action: mints a single-use token an agent process can redeem (within
/// `JOIN_TOKEN_TTL_SECONDS`) to register itself. Shown once in the Agents UI, same convention as
/// a secret's plaintext value never round-tripping back from the API after creation.
pub async fn create_join_token(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<Json<JoinTokenResponse>> {
    let mut token_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_bytes);
    let token = hex::encode(token_bytes);
    let token_hash = atk_auth::password::hash(&token).map_err(AppError::Internal)?;
    join_token_queries::create(&state.db, &token_hash, JOIN_TOKEN_TTL_SECONDS).await?;
    Ok(Json(JoinTokenResponse { token, expires_in_seconds: JOIN_TOKEN_TTL_SECONDS }))
}

#[derive(Deserialize)]
pub struct JoinRequest {
    pub token: String,
    pub name: String,
    pub os: String,
    pub arch: String,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Serialize)]
pub struct JoinResponse {
    pub agent_id: String,
    pub auth_token: String,
}

/// Agent action: redeems a join token (single-use; a replayed or expired token is
/// indistinguishable from a wrong one, `AppError::Unauthorized` either way) and registers as
/// `status = "pending"` â€” an operator still has to approve it from the Agents UI before the
/// scheduler will ever pick it for a job.
pub async fn join(State(state): State<AppState>, Json(req): Json<JoinRequest>) -> AppResult<Json<JoinResponse>> {
    // Hashing is one-way, so the presented token has to be checked against every still-valid
    // token hash rather than looked up directly; join tokens are short-lived and this table stays
    // small in practice (an operator mints one right before running the agent, not in bulk).
    let token_hash = atk_auth::password::hash(&req.token).map_err(AppError::Internal)?;
    let consumed = join_token_queries::consume(&state.db, &token_hash).await?;
    if !consumed {
        return Err(AppError::Unauthorized);
    }

    let mut auth_token_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut auth_token_bytes);
    let auth_token = hex::encode(auth_token_bytes);
    let auth_token_hash = atk_auth::password::hash(&auth_token).map_err(AppError::Internal)?;

    let labels_json = serde_json::to_string(&req.labels).unwrap_or_else(|_| "[]".to_string());
    let agent = agent_queries::create(&state.db, &req.name, &req.os, &req.arch, &labels_json, &auth_token_hash).await?;

    Ok(Json(JoinResponse { agent_id: agent.id, auth_token }))
}

pub async fn list(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<Json<Vec<Agent>>> {
    Ok(Json(agent_queries::list(&state.db).await?))
}

pub async fn approve(State(state): State<AppState>, Path(id): Path<String>, _user: ApprovedUser) -> AppResult<()> {
    agent_queries::find(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    agent_queries::set_status(&state.db, &id, "approved").await?;
    Ok(())
}

pub async fn revoke(State(state): State<AppState>, Path(id): Path<String>, _user: ApprovedUser) -> AppResult<()> {
    agent_queries::find(&state.db, &id).await?.ok_or(AppError::NotFound)?;
    agent_queries::set_status(&state.db, &id, "revoked").await?;
    Ok(())
}

async fn authenticate_agent(state: &AppState, agent_id: &str, headers: &HeaderMap) -> AppResult<Agent> {
    let agent = agent_queries::find(&state.db, agent_id).await?.ok_or(AppError::NotFound)?;
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    if agent.status == "revoked" || !atk_auth::password::verify(token, &agent.auth_token_hash) {
        return Err(AppError::Unauthorized);
    }
    Ok(agent)
}

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub capacity: i64,
    pub version: String,
}

pub async fn heartbeat(State(state): State<AppState>, Path(id): Path<String>, headers: HeaderMap, Json(req): Json<HeartbeatRequest>) -> AppResult<()> {
    authenticate_agent(&state, &id, &headers).await?;
    agent_queries::record_heartbeat(&state.db, &id, req.capacity, &req.version).await?;
    Ok(())
}

#[derive(Serialize)]
pub struct AssignedShell {
    pub shell_id: String,
    pub workflow_run_id: String,
}

pub async fn list_assignments(State(state): State<AppState>, Path(id): Path<String>, headers: HeaderMap) -> AppResult<Json<Vec<AssignedShell>>> {
    authenticate_agent(&state, &id, &headers).await?;
    let shells = shell_queries::list_assigned_for_agent(&state.db, &id).await?;
    Ok(Json(shells.into_iter().map(|s| AssignedShell { shell_id: s.id, workflow_run_id: s.workflow_run_id }).collect()))
}

/// Returns the raw `ShellRunSpec` JSON for one of this agent's assigned shells â€” deliberately
/// scoped to `shell.agent_id == id`, not just any authenticated agent, since the spec contains a
/// live checkout PAT.
pub async fn fetch_shell_spec(State(state): State<AppState>, Path((id, shell_id)): Path<(String, String)>, headers: HeaderMap) -> AppResult<String> {
    authenticate_agent(&state, &id, &headers).await?;
    let shell = shell_queries::find(&state.db, &shell_id).await?.ok_or(AppError::NotFound)?;
    if shell.agent_id.as_deref() != Some(id.as_str()) {
        return Err(AppError::Forbidden);
    }
    shell.spec_json.ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct ShellStartedRequest {
    pub pid: i64,
}

pub async fn shell_started(
    State(state): State<AppState>,
    Path((id, shell_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(req): Json<ShellStartedRequest>,
) -> AppResult<()> {
    authenticate_agent(&state, &id, &headers).await?;
    let shell = shell_queries::find(&state.db, &shell_id).await?.ok_or(AppError::NotFound)?;
    if shell.agent_id.as_deref() != Some(id.as_str()) {
        return Err(AppError::Forbidden);
    }
    shell_queries::mark_started(&state.db, &shell_id, req.pid).await?;
    Ok(())
}
