use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::error::AppResult;

#[derive(Serialize)]
pub struct AnalyticsSummary {
    pub total_runs: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub success_rate: f64,
    pub avg_duration_seconds: Option<f64>,
}

pub async fn summary(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<AnalyticsSummary>> {
    let row: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
            COUNT(*) as total, \
            SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END) as succeeded, \
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed, \
            SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END) as cancelled \
         FROM workflow_runs WHERE repo_id = ?",
    )
    .bind(&repo_id)
    .fetch_one(&state.db)
    .await?;

    let avg_duration: (Option<f64>,) = sqlx::query_as(
        "SELECT AVG((julianday(finished_at) - julianday(started_at)) * 86400.0) \
         FROM workflow_runs WHERE repo_id = ? AND started_at IS NOT NULL AND finished_at IS NOT NULL",
    )
    .bind(&repo_id)
    .fetch_one(&state.db)
    .await?;

    let (total, succeeded, failed, cancelled) = row;
    let success_rate = if total > 0 { succeeded as f64 / total as f64 } else { 0.0 };

    Ok(Json(AnalyticsSummary {
        total_runs: total,
        succeeded,
        failed,
        cancelled,
        success_rate,
        avg_duration_seconds: avg_duration.0,
    }))
}

#[derive(Deserialize)]
pub struct TrendQuery {
    days: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DurationTrendPoint {
    pub day: String,
    pub avg_duration_seconds: Option<f64>,
    pub run_count: i64,
}

pub async fn duration_trend(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    Query(q): Query<TrendQuery>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<DurationTrendPoint>>> {
    let days = q.days.unwrap_or(30);
    let points = sqlx::query_as::<_, DurationTrendPoint>(
        "SELECT \
            date(created_at) as day, \
            AVG((julianday(finished_at) - julianday(started_at)) * 86400.0) as avg_duration_seconds, \
            COUNT(*) as run_count \
         FROM workflow_runs \
         WHERE repo_id = ? AND created_at >= datetime('now', ? || ' days') \
         GROUP BY date(created_at) ORDER BY day ASC",
    )
    .bind(&repo_id)
    .bind(format!("-{days}"))
    .fetch_all(&state.db)
    .await?;

    Ok(Json(points))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

pub async fn status_breakdown(
    State(state): State<AppState>,
    Path(repo_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Vec<StatusCount>>> {
    let rows = sqlx::query_as::<_, StatusCount>(
        "SELECT status, COUNT(*) as count FROM workflow_runs WHERE repo_id = ? GROUP BY status",
    )
    .bind(&repo_id)
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rows))
}
