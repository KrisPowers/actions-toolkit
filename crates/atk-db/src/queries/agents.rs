use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, Agent};

pub async fn create(pool: &SqlitePool, name: &str, os: &str, arch: &str, labels_json: &str, mtls_fingerprint: &str) -> sqlx::Result<Agent> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO agents (id, name, os, arch, labels_json, mtls_fingerprint, status, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(os)
    .bind(arch)
    .bind(labels_json)
    .bind(mtls_fingerprint)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Agent>> {
    sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = ?").bind(id).fetch_optional(pool).await
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<Agent>> {
    sqlx::query_as::<_, Agent>("SELECT * FROM agents ORDER BY created_at DESC").fetch_all(pool).await
}

/// Agents an operator has approved and that have heartbeated recently enough to be considered
/// online, the scheduler's candidate pool for a job whose `runs_on` can't be satisfied locally.
pub async fn list_available(pool: &SqlitePool) -> sqlx::Result<Vec<Agent>> {
    sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE status IN ('approved', 'online') ORDER BY last_heartbeat_at DESC")
        .fetch_all(pool)
        .await
}

pub async fn set_status(pool: &SqlitePool, id: &str, status: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE agents SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn record_heartbeat(pool: &SqlitePool, id: &str, capacity: i64, version: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE agents SET last_heartbeat_at = ?, capacity = ?, version = ?, status = CASE WHEN status = 'approved' THEN 'online' ELSE status END, updated_at = ? WHERE id = ?",
    )
    .bind(now_iso())
    .bind(capacity)
    .bind(version)
    .bind(now_iso())
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
