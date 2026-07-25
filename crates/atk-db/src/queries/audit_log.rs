use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, AuditLogEntry};

/// Grouped into a struct rather than positional args: several of these fields are
/// `Option<&str>` back to back, and call sites are numerous enough (every workflow mutation, run
/// dispatch, and repo integration action) that named fields are worth it here to avoid silently
/// swapping two options of the same type.
pub struct NewAuditLogEntry<'a> {
    pub repo_id: &'a str,
    pub actor_id: Option<&'a str>,
    pub actor_login: Option<&'a str>,
    pub action: &'a str,
    pub target_type: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub summary: &'a str,
    pub metadata: Option<&'a str>,
}

pub async fn record(pool: &SqlitePool, entry: NewAuditLogEntry<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO audit_log (id, repo_id, actor_id, actor_login, action, target_type, target_id, summary, metadata, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(entry.repo_id)
    .bind(entry.actor_id)
    .bind(entry.actor_login)
    .bind(entry.action)
    .bind(entry.target_type)
    .bind(entry.target_id)
    .bind(entry.summary)
    .bind(entry.metadata)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_for_repo(pool: &SqlitePool, repo_id: &str, limit: i64, offset: i64) -> sqlx::Result<Vec<AuditLogEntry>> {
    sqlx::query_as::<_, AuditLogEntry>("SELECT * FROM audit_log WHERE repo_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?")
        .bind(repo_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}
