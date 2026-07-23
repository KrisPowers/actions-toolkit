use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::now_iso;

pub async fn create(pool: &SqlitePool, token_hash: &str, ttl_seconds: i64) -> sqlx::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(ttl_seconds)).to_rfc3339();
    sqlx::query("INSERT INTO agent_join_tokens (id, token_hash, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(token_hash)
        .bind(&now)
        .bind(&expires_at)
        .execute(pool)
        .await?;
    Ok(id)
}

/// Consumes a join token by hash if it exists, hasn't expired, and hasn't already been used.
/// Returns whether the consume succeeded; a caller sees `false` for an invalid, expired, or
/// already-used token without needing to distinguish which (a join attempt gets the same "invalid
/// token" response either way, so a stale or replayed token can't be used to enumerate state).
pub async fn consume(pool: &SqlitePool, token_hash: &str) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_join_tokens SET used_at = ? WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(now_iso())
    .bind(token_hash)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}
