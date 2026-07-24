use sqlx::SqlitePool;

use crate::models::RepoTunnel;

pub async fn get(pool: &SqlitePool, repo_id: &str) -> sqlx::Result<Option<RepoTunnel>> {
    sqlx::query_as::<_, RepoTunnel>("SELECT * FROM repo_tunnels WHERE repo_id = ?").bind(repo_id).fetch_optional(pool).await
}
