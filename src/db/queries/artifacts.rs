use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::models::{now_iso, Artifact};

pub async fn create(
    pool: &SqlitePool,
    workflow_run_id: &str,
    job_run_id: Option<&str>,
    name: &str,
    path_on_disk: &str,
    size_bytes: i64,
    content_type: Option<&str>,
) -> sqlx::Result<Artifact> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO artifacts (id, workflow_run_id, job_run_id, name, path_on_disk, size_bytes, content_type, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(workflow_run_id)
    .bind(job_run_id)
    .bind(name)
    .bind(path_on_disk)
    .bind(size_bytes)
    .bind(content_type)
    .bind(&now)
    .execute(pool)
    .await?;

    find_by_id(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Artifact>> {
    sqlx::query_as::<_, Artifact>("SELECT * FROM artifacts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_for_run(pool: &SqlitePool, workflow_run_id: &str) -> sqlx::Result<Vec<Artifact>> {
    sqlx::query_as::<_, Artifact>("SELECT * FROM artifacts WHERE workflow_run_id = ? ORDER BY created_at DESC")
        .bind(workflow_run_id)
        .fetch_all(pool)
        .await
}

pub async fn find_by_run_and_name(pool: &SqlitePool, workflow_run_id: &str, name: &str) -> sqlx::Result<Option<Artifact>> {
    sqlx::query_as::<_, Artifact>("SELECT * FROM artifacts WHERE workflow_run_id = ? AND name = ?")
        .bind(workflow_run_id)
        .bind(name)
        .fetch_optional(pool)
        .await
}
