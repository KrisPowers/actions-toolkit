use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, User};

pub async fn count(pool: &SqlitePool) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(pool).await?;
    Ok(row.0)
}

pub async fn create(pool: &SqlitePool, username: &str, password_hash: &str, role: &str) -> sqlx::Result<User> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(username)
    .bind(password_hash)
    .bind(role)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_by_id(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_by_username(pool: &SqlitePool, username: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list(pool: &SqlitePool) -> sqlx::Result<Vec<User>> {
    sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY created_at ASC")
        .fetch_all(pool)
        .await
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM users WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}

pub async fn create_session(pool: &SqlitePool, user_id: &str, ttl: chrono::Duration) -> sqlx::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires = now + ttl;
    sqlx::query(
        "INSERT INTO sessions (id, user_id, created_at, expires_at, revoked) VALUES (?, ?, ?, ?, 0)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(now.to_rfc3339())
    .bind(expires.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn revoke_session(pool: &SqlitePool, session_id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE sessions SET revoked = 1 WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn session_valid(pool: &SqlitePool, session_id: &str) -> sqlx::Result<bool> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT expires_at FROM sessions WHERE id = ? AND revoked = 0")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    Ok(match row {
        Some((expires_at,)) => crate::models::parse_iso(&expires_at) > chrono::Utc::now(),
        None => false,
    })
}
