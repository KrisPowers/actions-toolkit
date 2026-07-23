use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, Bucket};

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    trigger_kind: &str,
    webhook_event_id: Option<&str>,
    repo_id: &str,
    auth_token_hash: &str,
    rcp_endpoint: &str,
) -> sqlx::Result<Bucket> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO buckets (id, trigger_kind, webhook_event_id, repo_id, status, auth_token_hash, rcp_endpoint, created_at) \
         VALUES (?, ?, ?, ?, 'running', ?, ?, ?)",
    )
    .bind(&id)
    .bind(trigger_kind)
    .bind(webhook_event_id)
    .bind(repo_id)
    .bind(auth_token_hash)
    .bind(rcp_endpoint)
    .bind(&now)
    .execute(pool)
    .await?;

    find(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Bucket>> {
    sqlx::query_as::<_, Bucket>("SELECT * FROM buckets WHERE id = ?").bind(id).fetch_optional(pool).await
}

/// The open (not yet completed) bucket for a given webhook delivery, if one already exists. Used
/// so that N workflows matched by the same webhook delivery share one bucket (and therefore one
/// RCP server, one resource cache, one ephemeral key) instead of each getting its own.
pub async fn find_open_for_webhook_event(pool: &SqlitePool, webhook_event_id: &str) -> sqlx::Result<Option<Bucket>> {
    sqlx::query_as::<_, Bucket>(
        "SELECT * FROM buckets WHERE webhook_event_id = ? AND completed_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(webhook_event_id)
    .fetch_optional(pool)
    .await
}

/// Buckets for a repo, most recent first — the Overview page's "triggering events" list, one row
/// per push/PR/release/manual dispatch/etc. regardless of whether it came through a webhook.
/// When `workflow_id` is given, only buckets with at least one shell driving a run of that
/// workflow are returned (selecting a workflow in the Overview catalog filters this list).
pub async fn list_for_repo(pool: &SqlitePool, repo_id: &str, workflow_id: Option<&str>, limit: i64) -> sqlx::Result<Vec<Bucket>> {
    match workflow_id {
        Some(workflow_id) => {
            sqlx::query_as::<_, Bucket>(
                "SELECT DISTINCT b.* FROM buckets b \
                 JOIN shells s ON s.bucket_id = b.id \
                 JOIN workflow_runs wr ON wr.id = s.workflow_run_id \
                 WHERE b.repo_id = ? AND wr.workflow_id = ? \
                 ORDER BY b.created_at DESC LIMIT ?",
            )
            .bind(repo_id)
            .bind(workflow_id)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, Bucket>("SELECT * FROM buckets WHERE repo_id = ? ORDER BY created_at DESC LIMIT ?")
                .bind(repo_id)
                .bind(limit)
                .fetch_all(pool)
                .await
        }
    }
}

pub async fn mark_completed(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE buckets SET status = 'completed', completed_at = ? WHERE id = ?")
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_reaped(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE buckets SET reaped_at = ? WHERE id = ?").bind(now_iso()).bind(id).execute(pool).await?;
    Ok(())
}

/// Completed buckets nothing has reaped yet: every shell inside them has already been cleaned up
/// (see `shells::list_unreaped`), so the bucket-level scaffolding (resource cache directory, RCP
/// server) can be torn down too.
pub async fn list_completed_unreaped(pool: &SqlitePool) -> sqlx::Result<Vec<Bucket>> {
    sqlx::query_as::<_, Bucket>("SELECT * FROM buckets WHERE completed_at IS NOT NULL AND reaped_at IS NULL")
        .fetch_all(pool)
        .await
}

/// Every still-open bucket regardless of completion, used once at startup to find buckets whose
/// owning process crashed before it could mark them completed.
pub async fn list_unreaped(pool: &SqlitePool) -> sqlx::Result<Vec<Bucket>> {
    sqlx::query_as::<_, Bucket>("SELECT * FROM buckets WHERE reaped_at IS NULL").fetch_all(pool).await
}
