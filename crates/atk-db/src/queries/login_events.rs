use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, LoginEvent};

#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &SqlitePool,
    user_id: Option<&str>,
    github_login: Option<&str>,
    github_id: Option<i64>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    outcome: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO login_events (id, user_id, github_login, github_id, ip_address, user_agent, outcome, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(github_login)
    .bind(github_id)
    .bind(ip_address)
    .bind(user_agent)
    .bind(outcome)
    .bind(now_iso())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool, limit: i64, offset: i64) -> sqlx::Result<Vec<LoginEvent>> {
    sqlx::query_as::<_, LoginEvent>("SELECT * FROM login_events ORDER BY created_at DESC LIMIT ? OFFSET ?")
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
}
