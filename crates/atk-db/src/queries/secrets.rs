use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, Secret};

/// Creates or replaces the named secret for a repo (upsert on the `(repo_id, name)` unique
/// constraint), so re-adding a secret with the same name rotates its value instead of erroring.
pub async fn upsert(
    pool: &SqlitePool,
    repo_id: &str,
    name: &str,
    value_encrypted: &[u8],
    value_nonce: &[u8],
    created_by: &str,
) -> sqlx::Result<Secret> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO secrets (id, repo_id, name, value_encrypted, value_nonce, created_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(repo_id, name) DO UPDATE SET \
            value_encrypted = excluded.value_encrypted, \
            value_nonce = excluded.value_nonce, \
            updated_at = excluded.updated_at",
    )
    .bind(&id)
    .bind(repo_id)
    .bind(name)
    .bind(value_encrypted)
    .bind(value_nonce)
    .bind(created_by)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, Secret>("SELECT * FROM secrets WHERE repo_id = ? AND name = ?")
        .bind(repo_id)
        .bind(name)
        .fetch_one(pool)
        .await
}

pub async fn list_for_repo(pool: &SqlitePool, repo_id: &str) -> sqlx::Result<Vec<Secret>> {
    sqlx::query_as::<_, Secret>("SELECT * FROM secrets WHERE repo_id = ? ORDER BY name ASC")
        .bind(repo_id)
        .fetch_all(pool)
        .await
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Secret>> {
    sqlx::query_as::<_, Secret>("SELECT * FROM secrets WHERE id = ?").bind(id).fetch_optional(pool).await
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM secrets WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}
