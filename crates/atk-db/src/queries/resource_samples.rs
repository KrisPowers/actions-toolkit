use sqlx::SqlitePool;

use crate::models::ResourceSample;

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    subject_type: &str,
    subject_id: &str,
    workflow_run_id: Option<&str>,
    ts: &str,
    cpu_percent: Option<f64>,
    memory_bytes: Option<i64>,
    disk_read_bytes: Option<i64>,
    disk_write_bytes: Option<i64>,
    process_count: Option<i64>,
    host_cpu_percent: Option<f64>,
    host_memory_percent: Option<f64>,
) -> sqlx::Result<ResourceSample> {
    let id = sqlx::query(
        "INSERT INTO resource_samples \
         (subject_type, subject_id, workflow_run_id, ts, cpu_percent, memory_bytes, disk_read_bytes, disk_write_bytes, process_count, host_cpu_percent, host_memory_percent) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(subject_type)
    .bind(subject_id)
    .bind(workflow_run_id)
    .bind(ts)
    .bind(cpu_percent)
    .bind(memory_bytes)
    .bind(disk_read_bytes)
    .bind(disk_write_bytes)
    .bind(process_count)
    .bind(host_cpu_percent)
    .bind(host_memory_percent)
    .execute(pool)
    .await?
    .last_insert_rowid();

    sqlx::query_as::<_, ResourceSample>("SELECT * FROM resource_samples WHERE id = ?").bind(id).fetch_one(pool).await
}

/// Every sample for one subject (a shell or a shard), oldest first — a single node's full history
/// for chart population.
pub async fn list_for_subject(pool: &SqlitePool, subject_type: &str, subject_id: &str) -> sqlx::Result<Vec<ResourceSample>> {
    sqlx::query_as::<_, ResourceSample>(
        "SELECT * FROM resource_samples WHERE subject_type = ? AND subject_id = ? ORDER BY ts ASC",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_all(pool)
    .await
}

/// Every sample (shell and shard alike) tied to one workflow run, oldest first — what a run's
/// Backend tab loads on first paint before switching to the live websocket tail.
pub async fn list_for_run(pool: &SqlitePool, workflow_run_id: &str) -> sqlx::Result<Vec<ResourceSample>> {
    sqlx::query_as::<_, ResourceSample>("SELECT * FROM resource_samples WHERE workflow_run_id = ? ORDER BY ts ASC")
        .bind(workflow_run_id)
        .fetch_all(pool)
        .await
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SubjectPeak {
    pub peak_cpu_percent: Option<f64>,
    pub peak_memory_bytes: Option<i64>,
    pub avg_cpu_percent: Option<f64>,
}

/// Peak/average figures for one subject across its whole recorded history, the summary numbers a
/// run's stat cards show alongside the full time series.
pub async fn peak_for_subject(pool: &SqlitePool, subject_type: &str, subject_id: &str) -> sqlx::Result<SubjectPeak> {
    sqlx::query_as::<_, SubjectPeak>(
        "SELECT MAX(cpu_percent) as peak_cpu_percent, MAX(memory_bytes) as peak_memory_bytes, AVG(cpu_percent) as avg_cpu_percent \
         FROM resource_samples WHERE subject_type = ? AND subject_id = ?",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_one(pool)
    .await
}

/// Periodic-sweep retention: samples older than the cutoff are of no ongoing use once a run has
/// long since finished, and this table otherwise grows unbounded at a fixed sampling cadence.
pub async fn delete_older_than(pool: &SqlitePool, cutoff: &str) -> sqlx::Result<u64> {
    let result = sqlx::query("DELETE FROM resource_samples WHERE ts < ?").bind(cutoff).execute(pool).await?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("atk-resource-samples-test-{}", uuid::Uuid::new_v4()));
        crate::connect(&dir.join("test.db")).await.expect("db connect should succeed")
    }

    #[tokio::test]
    async fn insert_and_list_for_subject_round_trips() {
        let pool = test_pool().await;
        insert(&pool, "shell", "shell-1", None, "2026-01-01T00:00:00Z", Some(12.5), Some(1024), Some(0), Some(0), Some(3), Some(20.0), Some(40.0))
            .await
            .expect("insert should succeed");
        insert(&pool, "shell", "shell-1", None, "2026-01-01T00:00:02Z", Some(15.0), Some(2048), Some(0), Some(0), Some(3), Some(22.0), Some(41.0))
            .await
            .expect("insert should succeed");
        insert(&pool, "shell", "shell-2", None, "2026-01-01T00:00:00Z", Some(99.0), Some(9999), Some(0), Some(0), Some(1), Some(20.0), Some(40.0))
            .await
            .expect("insert should succeed");

        let samples = list_for_subject(&pool, "shell", "shell-1").await.expect("list should succeed");
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].ts, "2026-01-01T00:00:00Z");
        assert_eq!(samples[1].memory_bytes, Some(2048));

        let peak = peak_for_subject(&pool, "shell", "shell-1").await.expect("peak should succeed");
        assert_eq!(peak.peak_cpu_percent, Some(15.0));
        assert_eq!(peak.peak_memory_bytes, Some(2048));
    }

    #[tokio::test]
    async fn delete_older_than_only_removes_stale_rows() {
        let pool = test_pool().await;
        insert(&pool, "shell", "shell-1", None, "2020-01-01T00:00:00Z", None, None, None, None, None, None, None)
            .await
            .expect("insert should succeed");
        insert(&pool, "shell", "shell-1", None, "2999-01-01T00:00:00Z", None, None, None, None, None, None, None)
            .await
            .expect("insert should succeed");

        let deleted = delete_older_than(&pool, "2500-01-01T00:00:00Z").await.expect("delete should succeed");
        assert_eq!(deleted, 1);

        let remaining = list_for_subject(&pool, "shell", "shell-1").await.expect("list should succeed");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].ts, "2999-01-01T00:00:00Z");
    }
}
