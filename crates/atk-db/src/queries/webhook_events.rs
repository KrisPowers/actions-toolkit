use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, WebhookEvent};

#[allow(clippy::too_many_arguments)]
pub async fn record(
    pool: &SqlitePool,
    repo_id: Option<&str>,
    github_event: &str,
    delivery_id: Option<&str>,
    payload_json: &str,
    signature_valid: bool,
    matched_workflow_ids: &str,
) -> sqlx::Result<WebhookEvent> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO webhook_events (id, repo_id, github_event, delivery_id, payload_json, signature_valid, matched_workflow_ids, received_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(delivery_id) DO NOTHING",
    )
    .bind(&id)
    .bind(repo_id)
    .bind(github_event)
    .bind(delivery_id)
    .bind(payload_json)
    .bind(signature_valid as i64)
    .bind(matched_workflow_ids)
    .bind(&now)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, WebhookEvent>("SELECT * FROM webhook_events WHERE id = ? OR delivery_id = ?")
        .bind(&id)
        .bind(delivery_id)
        .fetch_one(pool)
        .await
}

pub async fn list_for_repo(pool: &SqlitePool, repo_id: &str, limit: i64) -> sqlx::Result<Vec<WebhookEvent>> {
    sqlx::query_as::<_, WebhookEvent>(
        "SELECT * FROM webhook_events WHERE repo_id = ? ORDER BY received_at DESC LIMIT ?",
    )
    .bind(repo_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}
