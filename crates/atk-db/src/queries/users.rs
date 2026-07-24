use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, User};

pub async fn count(pool: &SqlitePool) -> sqlx::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users").fetch_one(pool).await?;
    Ok(row.0)
}

pub async fn find_by_github_id(pool: &SqlitePool, github_id: i64) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE github_id = ?")
        .bind(github_id)
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

/// Inserts or updates the user row for a GitHub identity that just completed login.
/// `default_role`/`default_status` only apply the first time this `github_id` is seen; a
/// returning user keeps whatever role/status an admin has since set, and only has their
/// GitHub-sourced profile fields (login, display name, avatar) and `last_login_at`
/// refreshed.
pub async fn upsert_from_github(
    pool: &SqlitePool,
    github_id: i64,
    github_login: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    default_role: &str,
    default_status: &str,
) -> sqlx::Result<User> {
    let now = now_iso();
    if let Some(existing) = find_by_github_id(pool, github_id).await? {
        sqlx::query(
            "UPDATE users SET github_login = ?, display_name = ?, avatar_url = ?, updated_at = ?, last_login_at = ? WHERE id = ?",
        )
        .bind(github_login)
        .bind(display_name)
        .bind(avatar_url)
        .bind(&now)
        .bind(&now)
        .bind(&existing.id)
        .execute(pool)
        .await?;
        return find_by_id(pool, &existing.id).await?.ok_or(sqlx::Error::RowNotFound);
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, github_id, github_login, display_name, avatar_url, role, status, created_at, updated_at, last_login_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(github_id)
    .bind(github_login)
    .bind(display_name)
    .bind(avatar_url)
    .bind(default_role)
    .bind(default_status)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_by_id(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn set_status(pool: &SqlitePool, id: &str, status: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET status = ?, updated_at = ? WHERE id = ?")
        .bind(status)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_role(pool: &SqlitePool, id: &str, role: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE users SET role = ?, updated_at = ? WHERE id = ?")
        .bind(role)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
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
