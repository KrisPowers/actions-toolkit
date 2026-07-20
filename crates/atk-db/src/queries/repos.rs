use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, Repo};

pub async fn create(
    pool: &SqlitePool,
    owner: &str,
    name: &str,
    default_branch: &str,
    webhook_secret_encrypted: &[u8],
    webhook_secret_nonce: &[u8],
    created_by: &str,
) -> sqlx::Result<Repo> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO repos (id, owner, name, default_branch, webhook_secret_encrypted, webhook_secret_nonce, created_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(owner)
    .bind(name)
    .bind(default_branch)
    .bind(webhook_secret_encrypted)
    .bind(webhook_secret_nonce)
    .bind(created_by)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_by_id(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<Repo>> {
    sqlx::query_as::<_, Repo>("SELECT * FROM repos ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Repo>> {
    sqlx::query_as::<_, Repo>("SELECT * FROM repos WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM repos WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn set_github_hook_id(pool: &SqlitePool, id: &str, github_hook_id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE repos SET github_hook_id = ?, updated_at = ? WHERE id = ?")
        .bind(github_hook_id)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Repos without a working webhook (`github_hook_id` is `None`) are exactly the ones the polling
/// fallback needs to cover; a repo with a real webhook doesn't need polling too.
pub async fn list_without_webhook(pool: &SqlitePool) -> sqlx::Result<Vec<Repo>> {
    sqlx::query_as::<_, Repo>("SELECT * FROM repos WHERE github_hook_id IS NULL ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
}

pub async fn set_last_synced_release_id(pool: &SqlitePool, id: &str, release_id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE repos SET last_synced_release_id = ?, updated_at = ? WHERE id = ?")
        .bind(release_id)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
