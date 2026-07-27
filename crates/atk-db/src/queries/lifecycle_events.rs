use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, LifecycleEvent};

/// Opens a trip-wire timing for one phase of the runner's event pipeline (see the migration's own
/// comment for the full rationale). `finished_at` stays `NULL` until `finish` is called with the
/// returned id -- a row that never gets one is exactly the "still running or hung" signal this
/// table exists to catch. `started_at` is a caller-supplied timestamp rather than always "now"
/// so a phase that can only be reported *after* the fact (the RCP handshake itself, before there's
/// any channel to report through) can still record when it actually began, not just when this
/// call happened to run.
#[allow(clippy::too_many_arguments)]
pub async fn start(
    pool: &SqlitePool,
    phase: &str,
    subject_type: &str,
    subject_id: &str,
    workflow_run_id: Option<&str>,
    detail: Option<&str>,
    started_at: &str,
) -> sqlx::Result<LifecycleEvent> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO lifecycle_events (id, phase, subject_type, subject_id, workflow_run_id, detail, started_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(phase)
    .bind(subject_type)
    .bind(subject_id)
    .bind(workflow_run_id)
    .bind(detail)
    .bind(started_at)
    .execute(pool)
    .await?;

    find(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Closes a trip-wire timing opened by `start`. `ok` is `None` for the (rare) case a phase's
/// outcome genuinely isn't known (e.g. a process that was reaped out from under a wait rather than
/// exiting on its own); `Some(false)` + `finish_detail` covers an actual failure.
pub async fn finish(pool: &SqlitePool, id: &str, ok: Option<bool>, finish_detail: Option<&str>) -> sqlx::Result<()> {
    sqlx::query("UPDATE lifecycle_events SET finished_at = ?, ok = ?, finish_detail = ? WHERE id = ?")
        .bind(now_iso())
        .bind(ok.map(|b| b as i64))
        .bind(finish_detail)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn find(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<LifecycleEvent>> {
    sqlx::query_as::<_, LifecycleEvent>("SELECT * FROM lifecycle_events WHERE id = ?").bind(id).fetch_optional(pool).await
}

/// Every phase recorded for one workflow run, oldest first -- what powers the failed-run export's
/// timeline and lets a gap like "job started, first step didn't start for 9 minutes" be read
/// directly off phase boundaries instead of reconstructed by hand from job/step timestamps alone.
pub async fn list_for_run(pool: &SqlitePool, workflow_run_id: &str) -> sqlx::Result<Vec<LifecycleEvent>> {
    sqlx::query_as::<_, LifecycleEvent>("SELECT * FROM lifecycle_events WHERE workflow_run_id = ? ORDER BY started_at ASC")
        .bind(workflow_run_id)
        .fetch_all(pool)
        .await
}

/// Phases that started but never got a `finish` call, across every run -- the direct "what's
/// currently stuck" query, not just a post-mortem one. `older_than` filters out phases that are
/// simply still in flight (a build that's 30 seconds in isn't a hang); callers pick the threshold.
pub async fn list_unfinished_older_than(pool: &SqlitePool, older_than: &str) -> sqlx::Result<Vec<LifecycleEvent>> {
    sqlx::query_as::<_, LifecycleEvent>(
        "SELECT * FROM lifecycle_events WHERE finished_at IS NULL AND started_at < ? ORDER BY started_at ASC",
    )
    .bind(older_than)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("atk-lifecycle-events-test-{}", uuid::Uuid::new_v4()));
        crate::connect(&dir.join("test.db")).await.expect("db connect should succeed")
    }

    /// `workflow_run_id` is FK-constrained to a real `workflow_runs` row, so any test that wants
    /// one has to seed the full chain first -- mirrors `shards::tests::seed_fk_chain`.
    async fn seed_run(pool: &SqlitePool, repo_id: &str, run_id: &str) {
        let now = now_iso();
        let user_id = format!("user-{repo_id}");
        sqlx::query(
            "INSERT INTO users (id, github_id, github_login, role, status, created_at, updated_at) VALUES (?, ?, ?, 'admin', 'approved', ?, ?)",
        )
        .bind(&user_id)
        .bind(repo_id.as_bytes().iter().fold(0_i64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as i64)))
        .bind(format!("test-user-{repo_id}"))
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

        let workflow_id = format!("workflow-{repo_id}");
        sqlx::query(
            "INSERT INTO workflows (id, repo_id, name, file_path, yaml_source, parsed_json, enabled, created_at, updated_at) \
             VALUES (?, ?, 'test-workflow', 'ci.yml', '', '{}', 1, ?, ?)",
        )
        .bind(&workflow_id)
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
        .bind(&workflow_id)
        .bind(repo_id)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn start_then_finish_round_trips() {
        let pool = test_pool().await;
        seed_run(&pool, "repo-1", "run-1").await;
        let event = start(&pool, "checkout", "job", "job-1", Some("run-1"), None, "2026-01-01T00:00:00Z")
            .await
            .expect("start should succeed");
        assert!(event.finished_at.is_none());

        finish(&pool, &event.id, Some(true), None).await.expect("finish should succeed");

        let reloaded = find(&pool, &event.id).await.expect("find should succeed").expect("row should exist");
        assert!(reloaded.finished_at.is_some());
        assert_eq!(reloaded.ok, Some(1));
    }

    #[tokio::test]
    async fn list_for_run_only_returns_that_runs_phases_in_order() {
        let pool = test_pool().await;
        seed_run(&pool, "repo-1", "run-1").await;
        seed_run(&pool, "repo-2", "run-2").await;
        start(&pool, "bucket_create", "bucket", "bucket-1", None, None, "2026-01-01T00:00:00Z").await.unwrap();
        start(&pool, "checkout", "job", "job-1", Some("run-1"), None, "2026-01-01T00:00:02Z").await.unwrap();
        start(&pool, "shard_create", "job", "job-1", Some("run-1"), None, "2026-01-01T00:00:01Z").await.unwrap();
        start(&pool, "checkout", "job", "job-2", Some("run-2"), None, "2026-01-01T00:00:00Z").await.unwrap();

        let phases = list_for_run(&pool, "run-1").await.expect("list should succeed");
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].phase, "shard_create", "should be ordered by started_at, not insertion order");
        assert_eq!(phases[1].phase, "checkout");
    }

    #[tokio::test]
    async fn list_unfinished_older_than_excludes_finished_and_recent_rows() {
        let pool = test_pool().await;
        let stuck = start(&pool, "grant_ancestor_traverse", "shard", "shard-1", None, None, "2026-01-01T00:00:00Z").await.unwrap();
        let finished = start(&pool, "checkout", "job", "job-1", None, None, "2026-01-01T00:00:00Z").await.unwrap();
        finish(&pool, &finished.id, Some(true), None).await.unwrap();
        start(&pool, "step_exec", "step", "step-1", None, None, "2999-01-01T00:00:00Z").await.unwrap();

        let stuck_rows = list_unfinished_older_than(&pool, "2500-01-01T00:00:00Z").await.expect("query should succeed");
        assert_eq!(stuck_rows.len(), 1);
        assert_eq!(stuck_rows[0].id, stuck.id);
    }
}
