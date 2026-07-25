use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::middleware::ApprovedUser;
use crate::db::models::AuditLogEntry;
use crate::db::queries::audit_log as audit_log_queries;
use crate::error::AppResult;

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_for_repo(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Query(q): Query<ListQuery>,
    _user: ApprovedUser,
) -> AppResult<Json<Vec<AuditLogEntry>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    Ok(Json(audit_log_queries::list_for_repo(&state.db, &repo_id, limit, offset).await?))
}
