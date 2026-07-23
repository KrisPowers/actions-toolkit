use sqlx::SqlitePool;

use crate::models::{now_iso, Shard};

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    id: &str,
    job_run_id: &str,
    workflow_run_id: &str,
    workspace_path: &str,
    network_enabled: bool,
    ttl_expires_at: &str,
) -> sqlx::Result<Shard> {
    sqlx::query(
        "INSERT INTO shards (id, job_run_id, workflow_run_id, workspace_path, network_enabled, \
         created_at, ttl_expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(job_run_id)
    .bind(workflow_run_id)
    .bind(workspace_path)
    .bind(network_enabled as i64)
    .bind(now_iso())
    .bind(ttl_expires_at)
    .execute(pool)
    .await?;

    find(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Shard>> {
    sqlx::query_as::<_, Shard>("SELECT * FROM shards WHERE id = ?").bind(id).fetch_optional(pool).await
}

pub async fn set_os_handle(pool: &SqlitePool, id: &str, os_pid: i64, os_handle_json: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE shards SET os_pid = ?, os_handle_json = ? WHERE id = ?")
        .bind(os_pid)
        .bind(os_handle_json)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_reaped(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE shards SET reaped_at = ? WHERE id = ?")
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Shards whose TTL has already passed and that nothing has reaped yet, the periodic sweep
/// target.
pub async fn list_expired(pool: &SqlitePool) -> sqlx::Result<Vec<Shard>> {
    sqlx::query_as::<_, Shard>(
        "SELECT * FROM shards WHERE reaped_at IS NULL AND ttl_expires_at < ? ORDER BY ttl_expires_at ASC",
    )
    .bind(now_iso())
    .fetch_all(pool)
    .await
}

/// Every still-open shard regardless of TTL, used once at startup to find shards that
/// outlived a crash of the previous process and must be force-cleaned unconditionally.
pub async fn list_unreaped(pool: &SqlitePool) -> sqlx::Result<Vec<Shard>> {
    sqlx::query_as::<_, Shard>("SELECT * FROM shards WHERE reaped_at IS NULL").fetch_all(pool).await
}

/// Still-open shards belonging to a given run, the cancel handler's target, so a cancelled
/// run's shards are torn down immediately instead of waiting for the TTL reaper.
pub async fn list_unreaped_for_run(pool: &SqlitePool, workflow_run_id: &str) -> sqlx::Result<Vec<Shard>> {
    sqlx::query_as::<_, Shard>("SELECT * FROM shards WHERE reaped_at IS NULL AND workflow_run_id = ?")
        .bind(workflow_run_id)
        .fetch_all(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_fk_chain(pool: &SqlitePool, repo_id: &str, workflow_id: &str, run_id: &str, job_run_id: &str) {
        let now = now_iso();
        let user_id = format!("user-{repo_id}");
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, 'admin', ?, ?)",
        )
        .bind(&user_id)
        .bind(format!("test-user-{repo_id}"))
        .bind("hash")
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO repos (id, owner, name, default_branch, webhook_secret_encrypted, \
             webhook_secret_nonce, created_by, created_at, updated_at) VALUES (?, 'test-owner', ?, 'main', \
             x'00', x'00', ?, ?, ?)",
        )
        .bind(repo_id)
        .bind(repo_id)
        .bind(&user_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO workflows (id, repo_id, name, file_path, yaml_source, parsed_json, enabled, created_at, updated_at) \
             VALUES (?, ?, 'test-workflow', 'ci.yml', '', '{}', 1, ?, ?)",
        )
        .bind(workflow_id)
        .bind(repo_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO workflow_runs (id, workflow_id, repo_id, trigger_event, status, created_at) \
             VALUES (?, ?, ?, 'manual', 'running', ?)",
        )
        .bind(run_id)
        .bind(workflow_id)
        .bind(repo_id)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO job_runs (id, workflow_run_id, job_key, status) VALUES (?, ?, 'build', 'running')")
            .bind(job_run_id)
            .bind(run_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("atk-bucket-queries-test-{}", uuid::Uuid::new_v4()));
        crate::connect(&dir.join("test.db")).await.expect("db connect should succeed")
    }

    #[tokio::test]
    async fn list_unreaped_for_run_only_returns_open_buckets_for_that_run() {
        let pool = test_pool().await;
        seed_fk_chain(&pool, "repo-1", "workflow-1", "run-1", "job-1").await;
        seed_fk_chain(&pool, "repo-2", "workflow-2", "run-2", "job-2").await;

        let in_run = create(&pool, "bucket-1", "job-1", "run-1", "/workspace/1", false, "2999-01-01T00:00:00Z")
            .await
            .expect("create should succeed");
        let already_reaped = create(&pool, "bucket-2", "job-1", "run-1", "/workspace/2", false, "2999-01-01T00:00:00Z")
            .await
            .expect("create should succeed");
        mark_reaped(&pool, &already_reaped.id).await.expect("mark_reaped should succeed");
        create(&pool, "bucket-3", "job-2", "run-2", "/workspace/3", false, "2999-01-01T00:00:00Z")
            .await
            .expect("create should succeed");

        let found = list_unreaped_for_run(&pool, "run-1").await.expect("list_unreaped_for_run should succeed");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, in_run.id);
    }
}
