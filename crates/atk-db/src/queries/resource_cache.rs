use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, BucketResourceCacheEntry};

pub async fn find(pool: &SqlitePool, bucket_id: &str, cache_key: &str) -> sqlx::Result<Option<BucketResourceCacheEntry>> {
    sqlx::query_as::<_, BucketResourceCacheEntry>("SELECT * FROM bucket_resource_cache WHERE bucket_id = ? AND cache_key = ?")
        .bind(bucket_id)
        .bind(cache_key)
        .fetch_optional(pool)
        .await
}

/// Atomically claims the build lease for `cache_key`, or reports who already holds it. Two
/// concurrent shells calling this for the same key race on the `UNIQUE(bucket_id, cache_key)`
/// constraint: exactly one `INSERT` wins, the other gets a constraint violation and falls through
/// to reading back whichever row actually landed. Returns the row regardless of who won; the
/// caller compares `builder_shell_id` against its own shell id to tell which case it's in.
pub async fn begin_build(
    pool: &SqlitePool,
    bucket_id: &str,
    cache_key: &str,
    builder_shell_id: &str,
) -> sqlx::Result<BucketResourceCacheEntry> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let insert = sqlx::query(
        "INSERT INTO bucket_resource_cache (id, bucket_id, cache_key, status, builder_shell_id, builder_heartbeat_at, created_at) \
         VALUES (?, ?, ?, 'building', ?, ?, ?)",
    )
    .bind(&id)
    .bind(bucket_id)
    .bind(cache_key)
    .bind(builder_shell_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await;

    match insert {
        Ok(_) => find(pool, bucket_id, cache_key).await?.ok_or(sqlx::Error::RowNotFound),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            find(pool, bucket_id, cache_key).await?.ok_or(sqlx::Error::RowNotFound)
        }
        Err(e) => Err(e),
    }
}

pub async fn heartbeat_build(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE bucket_resource_cache SET builder_heartbeat_at = ? WHERE id = ? AND status = 'building'")
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn complete_build(pool: &SqlitePool, id: &str, path_on_disk: &str, size_bytes: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE bucket_resource_cache SET status = 'ready', path_on_disk = ?, size_bytes = ?, ready_at = ? WHERE id = ?",
    )
    .bind(path_on_disk)
    .bind(size_bytes)
    .bind(now_iso())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn fail_build(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE bucket_resource_cache SET status = 'failed', failed_at = ? WHERE id = ?")
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Resets a build lease whose builder went silent (no heartbeat within `stale_after_seconds`)
/// back to a clean slate by deleting the row outright, so the next shell to ask for this
/// `cache_key` sees a plain miss and starts a fresh build, rather than a scheme with an explicit
/// "reset" status that every reader would also need to treat as a miss.
pub async fn reap_stale_builds(pool: &SqlitePool, stale_after_seconds: i64) -> sqlx::Result<u64> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(stale_after_seconds)).to_rfc3339();
    let result = sqlx::query("DELETE FROM bucket_resource_cache WHERE status = 'building' AND builder_heartbeat_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
