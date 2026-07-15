use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::models::{now_iso, Repo};

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    owner: &str,
    name: &str,
    default_branch: &str,
    pat_encrypted: &[u8],
    pat_nonce: &[u8],
    webhook_secret_encrypted: &[u8],
    webhook_secret_nonce: &[u8],
    created_by: &str,
) -> sqlx::Result<Repo> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO repos (id, owner, name, default_branch, pat_encrypted, pat_nonce, \
         webhook_secret_encrypted, webhook_secret_nonce, created_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(owner)
    .bind(name)
    .bind(default_branch)
    .bind(pat_encrypted)
    .bind(pat_nonce)
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

pub async fn update_pat(
    pool: &SqlitePool,
    id: &str,
    pat_encrypted: &[u8],
    pat_nonce: &[u8],
) -> sqlx::Result<()> {
    sqlx::query("UPDATE repos SET pat_encrypted = ?, pat_nonce = ?, updated_at = ? WHERE id = ?")
        .bind(pat_encrypted)
        .bind(pat_nonce)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM repos WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}
