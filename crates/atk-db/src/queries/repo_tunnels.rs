use sqlx::SqlitePool;

use crate::models::RepoTunnel;

pub async fn get(pool: &SqlitePool, repo_id: &str) -> sqlx::Result<Option<RepoTunnel>> {
    sqlx::query_as::<_, RepoTunnel>("SELECT * FROM repo_tunnels WHERE repo_id = ?").bind(repo_id).fetch_optional(pool).await
}

/// Repos with a tunnel persisted as `enabled = 1`, so the ones that were running before the last
/// restart can be started again automatically instead of the operator re-clicking "Start" once
/// per repo (see the boot-time auto-start loop in `main.rs`).
pub async fn list_enabled(pool: &SqlitePool) -> sqlx::Result<Vec<RepoTunnel>> {
    sqlx::query_as::<_, RepoTunnel>("SELECT * FROM repo_tunnels WHERE enabled = 1").fetch_all(pool).await
}
