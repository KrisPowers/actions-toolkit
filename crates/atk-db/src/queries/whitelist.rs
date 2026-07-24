use sqlx::SqlitePool;

use crate::models::{now_iso, WhitelistEntry};

pub async fn add(pool: &SqlitePool, github_login: &str, added_by: Option<&str>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO github_whitelist (github_login, added_by, created_at) VALUES (?, ?, ?) \
         ON CONFLICT(github_login) DO NOTHING",
    )
    .bind(github_login)
    .bind(added_by)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove(pool: &SqlitePool, github_login: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM github_whitelist WHERE github_login = ?")
        .bind(github_login)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<WhitelistEntry>> {
    sqlx::query_as::<_, WhitelistEntry>("SELECT * FROM github_whitelist ORDER BY created_at ASC")
        .fetch_all(pool)
        .await
}

pub async fn is_whitelisted(pool: &SqlitePool, github_login: &str) -> sqlx::Result<bool> {
    let row: Option<(String,)> = sqlx::query_as("SELECT github_login FROM github_whitelist WHERE github_login = ? COLLATE NOCASE")
        .bind(github_login)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}
