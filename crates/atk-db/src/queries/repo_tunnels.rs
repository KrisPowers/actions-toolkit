use sqlx::SqlitePool;

use crate::models::{now_iso, RepoTunnel};

pub async fn get(pool: &SqlitePool, repo_id: &str) -> sqlx::Result<Option<RepoTunnel>> {
    sqlx::query_as::<_, RepoTunnel>("SELECT * FROM repo_tunnels WHERE repo_id = ?").bind(repo_id).fetch_optional(pool).await
}

/// Repos with a tunnel persisted as `enabled = 1`, so the ones that were running before the last
/// restart can be started again automatically instead of the operator re-clicking "Start" once
/// per repo (see the boot-time auto-start loop in `main.rs`).
pub async fn list_enabled(pool: &SqlitePool) -> sqlx::Result<Vec<RepoTunnel>> {
    sqlx::query_as::<_, RepoTunnel>("SELECT * FROM repo_tunnels WHERE enabled = 1").fetch_all(pool).await
}

pub async fn upsert(pool: &SqlitePool, repo_id: &str, provider: &str, enabled: bool, local_port: Option<i64>, last_url: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO repo_tunnels (repo_id, provider, enabled, local_port, last_url, updated_at) VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(repo_id) DO UPDATE SET provider = excluded.provider, enabled = excluded.enabled, \
         local_port = excluded.local_port, last_url = excluded.last_url, updated_at = excluded.updated_at",
    )
    .bind(repo_id)
    .bind(provider)
    .bind(enabled as i64)
    .bind(local_port)
    .bind(last_url)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}
