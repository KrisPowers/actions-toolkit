//! TTL-based auto-cleanup and startup crash reconciliation for Bucket sandboxes. Deliberately
//! OS-agnostic: it only talks to the `buckets` DB table and calls the shared `remove_bucket`,
//! leaving "how do I actually force-kill this" to the OS-specific backend.

use std::path::Path;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::db::queries::buckets as bucket_queries;

const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Startup-only pass: force-clean every bucket row still open when the process starts, since a
/// bucket row with no `reaped_at` at startup can only mean the previous process died before it
/// could tear the sandbox down itself (there is no other path that leaves a row open).
pub async fn reconcile_on_startup(pool: &SqlitePool, buckets_root: &Path) {
    let rows = match bucket_queries::list_unreaped(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "failed to list unreaped buckets at startup");
            return;
        }
    };

    for row in rows {
        tracing::warn!(bucket_id = %row.id, "cleaning up bucket left over from a previous process");
        let handle = super::handle_from_bucket_row(buckets_root, &row);
        if let Err(e) = super::remove_bucket(pool, &handle).await {
            tracing::warn!(error = %e, bucket_id = %row.id, "failed to force-clean leftover bucket");
        }
        if let Err(e) = bucket_queries::mark_reaped(pool, &row.id).await {
            tracing::warn!(error = %e, bucket_id = %row.id, "failed to mark leftover bucket reaped");
        }
    }
}

/// Periodic sweep: force-clean any bucket whose TTL has passed but that normal job completion
/// hasn't reaped yet (e.g. a step wedged past its expected runtime).
pub async fn run_periodic_sweep(pool: SqlitePool, buckets_root: Arc<Path>) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    loop {
        interval.tick().await;
        let rows = match bucket_queries::list_expired(&pool).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(error = %e, "failed to list expired buckets");
                continue;
            }
        };
        for row in rows {
            tracing::warn!(bucket_id = %row.id, "bucket exceeded its TTL, force-cleaning");
            let handle = super::handle_from_bucket_row(&buckets_root, &row);
            if let Err(e) = super::remove_bucket(&pool, &handle).await {
                tracing::warn!(error = %e, bucket_id = %row.id, "failed to force-clean expired bucket");
            }
            if let Err(e) = bucket_queries::mark_reaped(&pool, &row.id).await {
                tracing::warn!(error = %e, bucket_id = %row.id, "failed to mark expired bucket reaped");
            }
        }
    }
}
