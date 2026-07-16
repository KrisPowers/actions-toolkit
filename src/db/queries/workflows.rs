use sqlx::SqlitePool;
use uuid::Uuid;

use crate::db::models::{now_iso, Workflow};

pub async fn create(
    pool: &SqlitePool,
    repo_id: &str,
    name: &str,
    file_path: &str,
    yaml_source: &str,
    parsed_json: &str,
) -> sqlx::Result<Workflow> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO workflows (id, repo_id, name, file_path, yaml_source, parsed_json, enabled, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&id)
    .bind(repo_id)
    .bind(name)
    .bind(file_path)
    .bind(yaml_source)
    .bind(parsed_json)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    find_by_id(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn list_for_repo(pool: &SqlitePool, repo_id: &str) -> sqlx::Result<Vec<Workflow>> {
    sqlx::query_as::<_, Workflow>("SELECT * FROM workflows WHERE repo_id = ? ORDER BY created_at DESC")
        .bind(repo_id)
        .fetch_all(pool)
        .await
}

pub async fn list_enabled_for_repo(pool: &SqlitePool, repo_id: &str) -> sqlx::Result<Vec<Workflow>> {
    sqlx::query_as::<_, Workflow>("SELECT * FROM workflows WHERE repo_id = ? AND enabled = 1")
        .bind(repo_id)
        .fetch_all(pool)
        .await
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Workflow>> {
    sqlx::query_as::<_, Workflow>("SELECT * FROM workflows WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    yaml_source: &str,
    parsed_json: &str,
) -> sqlx::Result<()> {
    sqlx::query("UPDATE workflows SET yaml_source = ?, parsed_json = ?, updated_at = ? WHERE id = ?")
        .bind(yaml_source)
        .bind(parsed_json)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_enabled(pool: &SqlitePool, id: &str, enabled: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE workflows SET enabled = ?, updated_at = ? WHERE id = ?")
        .bind(enabled as i64)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM workflows WHERE id = ?").bind(id).execute(pool).await?;
    Ok(())
}
