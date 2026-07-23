//! Read-only endpoints for the run/bucket "Backend" topology and runtime-resource insights: the
//! REST side of the observability pipeline `runner::sampler` feeds (see `crate::ws::run_stats_ws`
//! for the live-tail half). Every handler here only reads rows other code already wrote; nothing
//! in this module touches an RCP connection or a running shell/shard directly.

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::db::models::{Bucket, ResourceSample, Shard, Shell};
use crate::db::queries::{
    buckets as bucket_queries, resource_cache as cache_queries, resource_samples as sample_queries,
    runs as run_queries, shards as shard_queries, shells as shell_queries,
};
use crate::error::{AppError, AppResult};

#[derive(Serialize)]
pub struct BucketSummary {
    pub bucket: Bucket,
    /// Distinct resources this bucket's shells have successfully cached (a count only — never the
    /// cache keys, paths, or contents themselves).
    pub assets_cached: i64,
    pub shell_count: i64,
}

#[derive(Serialize)]
pub struct ShellNode {
    pub shell: Shell,
    pub shards: Vec<Shard>,
}

#[derive(Serialize)]
pub struct RunTopology {
    /// `None` only for a run whose shell hasn't been recorded yet (still queued) or whose bucket
    /// row is otherwise missing.
    pub bucket: Option<BucketSummary>,
    /// `None` for a run still queued, before its shell has been spawned.
    pub shell: Option<ShellNode>,
}

async fn bucket_summary(state: &AppState, bucket_id: &str) -> AppResult<Option<BucketSummary>> {
    let Some(bucket) = bucket_queries::find(&state.db, bucket_id).await? else { return Ok(None) };
    let assets_cached = cache_queries::count_ready_for_bucket(&state.db, bucket_id).await?;
    let shell_count = shell_queries::list_for_bucket(&state.db, bucket_id).await?.len() as i64;
    Ok(Some(BucketSummary { bucket, assets_cached, shell_count }))
}

pub async fn topology_for_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<RunTopology>> {
    run_queries::find_run(&state.db, &run_id).await?.ok_or(AppError::NotFound)?;

    let Some(shell) = shell_queries::find_by_workflow_run(&state.db, &run_id).await? else {
        return Ok(Json(RunTopology { bucket: None, shell: None }));
    };
    let shards = shard_queries::list_for_workflow_run(&state.db, &run_id).await?;
    let bucket = bucket_summary(&state, &shell.bucket_id).await?;

    Ok(Json(RunTopology { bucket, shell: Some(ShellNode { shell, shards }) }))
}

#[derive(Serialize)]
pub struct RunStatsSummary {
    pub samples: Vec<ResourceSample>,
    pub cache_hits: i64,
    pub cache_misses: i64,
    pub assets_cached: i64,
    pub peak_cpu_percent: Option<f64>,
    pub peak_memory_bytes: Option<i64>,
}

pub async fn stats_for_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<RunStatsSummary>> {
    run_queries::find_run(&state.db, &run_id).await?.ok_or(AppError::NotFound)?;

    let samples = sample_queries::list_for_run(&state.db, &run_id).await?;
    let shell = shell_queries::find_by_workflow_run(&state.db, &run_id).await?;

    let (cache_hits, cache_misses) = shell.as_ref().map(|s| (s.cache_hits, s.cache_misses)).unwrap_or((0, 0));
    let assets_cached = match &shell {
        Some(s) => cache_queries::count_ready_for_bucket(&state.db, &s.bucket_id).await?,
        None => 0,
    };

    let peak_cpu_percent = samples.iter().filter_map(|s| s.cpu_percent).fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.max(v))));
    let peak_memory_bytes = samples.iter().filter_map(|s| s.memory_bytes).max();

    Ok(Json(RunStatsSummary { samples, cache_hits, cache_misses, assets_cached, peak_cpu_percent, peak_memory_bytes }))
}

/// The bucket behind one webhook delivery, if any — powers the Runs page's "View backend" link.
/// `None` (not a 404) when the delivery matched no workflow and so never got a bucket at all.
pub async fn bucket_for_webhook_event(
    State(state): State<AppState>,
    Path(event_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<Option<Bucket>>> {
    let bucket = bucket_queries::find_by_webhook_event(&state.db, &event_id).await?;
    Ok(Json(bucket))
}

#[derive(Serialize)]
pub struct BucketTopology {
    pub bucket: BucketSummary,
    pub shells: Vec<ShellNode>,
}

pub async fn topology_for_bucket(
    State(state): State<AppState>,
    Path(bucket_id): Path<String>,
    _user: CurrentUser,
) -> AppResult<Json<BucketTopology>> {
    let summary = bucket_summary(&state, &bucket_id).await?.ok_or(AppError::NotFound)?;

    let mut shells = Vec::new();
    for shell in shell_queries::list_for_bucket(&state.db, &bucket_id).await? {
        let shards = shard_queries::list_for_workflow_run(&state.db, &shell.workflow_run_id).await?;
        shells.push(ShellNode { shell, shards });
    }

    Ok(Json(BucketTopology { bucket: summary, shells }))
}
