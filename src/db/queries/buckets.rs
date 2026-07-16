use sqlx::SqlitePool;

use crate::db::models::{now_iso, Bucket};

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    id: &str,
    job_run_id: &str,
    workflow_run_id: &str,
    workspace_path: &str,
    network_enabled: bool,
    ttl_expires_at: &str,
) -> sqlx::Result<Bucket> {
    sqlx::query(
        "INSERT INTO buckets (id, job_run_id, workflow_run_id, workspace_path, network_enabled, \
         created_at, ttl_expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(job_run_id)
    .bind(workflow_run_id)
    .bind(workspace_path)
    .bind(network_enabled as i64)
    .bind(now_iso())
    .bind(ttl_expires_at)
    .execute(pool)
    .await?;

    find(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Bucket>> {
    sqlx::query_as::<_, Bucket>("SELECT * FROM buckets WHERE id = ?").bind(id).fetch_optional(pool).await
}

pub async fn set_os_handle(pool: &SqlitePool, id: &str, os_pid: i64, os_handle_json: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE buckets SET os_pid = ?, os_handle_json = ? WHERE id = ?")
        .bind(os_pid)
        .bind(os_handle_json)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_reaped(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE buckets SET reaped_at = ? WHERE id = ?")
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Buckets whose TTL has already passed and that nothing has reaped yet — the periodic sweep
/// target.
pub async fn list_expired(pool: &SqlitePool) -> sqlx::Result<Vec<Bucket>> {
    sqlx::query_as::<_, Bucket>(
        "SELECT * FROM buckets WHERE reaped_at IS NULL AND ttl_expires_at < ? ORDER BY ttl_expires_at ASC",
    )
    .bind(now_iso())
    .fetch_all(pool)
    .await
}

/// Every still-open bucket regardless of TTL — used once at startup to find sandboxes that
/// outlived a crash of the previous process and must be force-cleaned unconditionally.
pub async fn list_unreaped(pool: &SqlitePool) -> sqlx::Result<Vec<Bucket>> {
    sqlx::query_as::<_, Bucket>("SELECT * FROM buckets WHERE reaped_at IS NULL").fetch_all(pool).await
}
