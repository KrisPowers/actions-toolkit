use sqlx::SqlitePool;

use crate::db::models::{now_iso, GithubToken};

pub async fn get(pool: &SqlitePool) -> sqlx::Result<Option<GithubToken>> {
    sqlx::query_as::<_, GithubToken>("SELECT * FROM github_token WHERE id = 1")
        .fetch_optional(pool)
        .await
}

pub async fn upsert(
    pool: &SqlitePool,
    token_encrypted: &[u8],
    token_nonce: &[u8],
    github_login: &str,
    scopes: &str,
) -> sqlx::Result<GithubToken> {
    let now = now_iso();
    sqlx::query(
        "INSERT INTO github_token (id, token_encrypted, token_nonce, github_login, scopes, created_at, updated_at) \
         VALUES (1, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
            token_encrypted = excluded.token_encrypted, \
            token_nonce = excluded.token_nonce, \
            github_login = excluded.github_login, \
            scopes = excluded.scopes, \
            updated_at = excluded.updated_at",
    )
    .bind(token_encrypted)
    .bind(token_nonce)
    .bind(github_login)
    .bind(scopes)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn delete(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM github_token WHERE id = 1").execute(pool).await?;
    Ok(())
}
