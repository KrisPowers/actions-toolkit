use sqlx::SqlitePool;
use uuid::Uuid;

use crate::models::{now_iso, JobRun, JobRunTree, RunLog, RunTree, StepRun, WorkflowRun};

#[allow(clippy::too_many_arguments)]
pub async fn create_run(
    pool: &SqlitePool,
    workflow_id: &str,
    repo_id: &str,
    trigger_event: &str,
    trigger_payload_json: Option<&str>,
    ref_name: Option<&str>,
    commit_sha: Option<&str>,
    webhook_event_id: Option<&str>,
) -> sqlx::Result<WorkflowRun> {
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO workflow_runs (id, workflow_id, repo_id, trigger_event, trigger_payload_json, \
         ref_name, commit_sha, status, created_at, webhook_event_id) VALUES (?, ?, ?, ?, ?, ?, ?, 'queued', ?, ?)",
    )
    .bind(&id)
    .bind(workflow_id)
    .bind(repo_id)
    .bind(trigger_event)
    .bind(trigger_payload_json)
    .bind(ref_name)
    .bind(commit_sha)
    .bind(&now)
    .bind(webhook_event_id)
    .execute(pool)
    .await?;

    find_run(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_run(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<WorkflowRun>> {
    sqlx::query_as::<_, WorkflowRun>("SELECT * FROM workflow_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_runs_for_repo(pool: &SqlitePool, repo_id: &str, limit: i64) -> sqlx::Result<Vec<WorkflowRun>> {
    sqlx::query_as::<_, WorkflowRun>(
        "SELECT * FROM workflow_runs WHERE repo_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(repo_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn list_for_webhook_event(pool: &SqlitePool, webhook_event_id: &str) -> sqlx::Result<Vec<WorkflowRun>> {
    sqlx::query_as::<_, WorkflowRun>(
        "SELECT * FROM workflow_runs WHERE webhook_event_id = ? ORDER BY created_at DESC",
    )
    .bind(webhook_event_id)
    .fetch_all(pool)
    .await
}

pub async fn set_run_status(pool: &SqlitePool, id: &str, status: &str, terminal: bool) -> sqlx::Result<()> {
    if terminal {
        sqlx::query("UPDATE workflow_runs SET status = ?, finished_at = ? WHERE id = ?")
            .bind(status)
            .bind(now_iso())
            .bind(id)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("UPDATE workflow_runs SET status = ?, started_at = COALESCE(started_at, ?) WHERE id = ?")
            .bind(status)
            .bind(now_iso())
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn create_job_run(
    pool: &SqlitePool,
    workflow_run_id: &str,
    job_key: &str,
    name: Option<&str>,
    needs_json: &str,
) -> sqlx::Result<JobRun> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO job_runs (id, workflow_run_id, job_key, name, status, needs_json) \
         VALUES (?, ?, ?, ?, 'pending', ?)",
    )
    .bind(&id)
    .bind(workflow_run_id)
    .bind(job_key)
    .bind(name)
    .bind(needs_json)
    .execute(pool)
    .await?;

    find_job_run(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_job_run(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<JobRun>> {
    sqlx::query_as::<_, JobRun>("SELECT * FROM job_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_job_runs(pool: &SqlitePool, workflow_run_id: &str) -> sqlx::Result<Vec<JobRun>> {
    sqlx::query_as::<_, JobRun>("SELECT * FROM job_runs WHERE workflow_run_id = ?")
        .bind(workflow_run_id)
        .fetch_all(pool)
        .await
}

pub async fn set_job_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    exit_code: Option<i64>,
    terminal: bool,
) -> sqlx::Result<()> {
    if terminal {
        sqlx::query(
            "UPDATE job_runs SET status = ?, exit_code = ?, finished_at = ? WHERE id = ?",
        )
        .bind(status)
        .bind(exit_code)
        .bind(now_iso())
        .bind(id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query("UPDATE job_runs SET status = ?, started_at = COALESCE(started_at, ?) WHERE id = ?")
            .bind(status)
            .bind(now_iso())
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn set_job_container(pool: &SqlitePool, id: &str, container_id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE job_runs SET container_id = ? WHERE id = ?")
        .bind(container_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn create_step_run(
    pool: &SqlitePool,
    job_run_id: &str,
    step_index: i64,
    name: Option<&str>,
    kind: &str,
) -> sqlx::Result<StepRun> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO step_runs (id, job_run_id, step_index, name, kind, status) \
         VALUES (?, ?, ?, ?, ?, 'pending')",
    )
    .bind(&id)
    .bind(job_run_id)
    .bind(step_index)
    .bind(name)
    .bind(kind)
    .execute(pool)
    .await?;

    find_step_run(pool, &id).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn find_step_run(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<StepRun>> {
    sqlx::query_as::<_, StepRun>("SELECT * FROM step_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

pub async fn list_step_runs(pool: &SqlitePool, job_run_id: &str) -> sqlx::Result<Vec<StepRun>> {
    sqlx::query_as::<_, StepRun>("SELECT * FROM step_runs WHERE job_run_id = ? ORDER BY step_index ASC")
        .bind(job_run_id)
        .fetch_all(pool)
        .await
}

pub async fn set_step_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    exit_code: Option<i64>,
    terminal: bool,
) -> sqlx::Result<()> {
    if terminal {
        sqlx::query("UPDATE step_runs SET status = ?, exit_code = ?, finished_at = ? WHERE id = ?")
            .bind(status)
            .bind(exit_code)
            .bind(now_iso())
            .bind(id)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("UPDATE step_runs SET status = ?, started_at = COALESCE(started_at, ?) WHERE id = ?")
            .bind(status)
            .bind(now_iso())
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn insert_log_lines(
    pool: &SqlitePool,
    lines: &[(String, String, String, String)], // (step_run_id, ts, stream, message)
) -> sqlx::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for (step_run_id, ts, stream, message) in lines {
        sqlx::query("INSERT INTO run_logs (step_run_id, ts, stream, message) VALUES (?, ?, ?, ?)")
            .bind(step_run_id)
            .bind(ts)
            .bind(stream)
            .bind(message)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_logs_for_run(pool: &SqlitePool, workflow_run_id: &str, since_id: i64) -> sqlx::Result<Vec<RunLog>> {
    sqlx::query_as::<_, RunLog>(
        "SELECT rl.* FROM run_logs rl \
         JOIN step_runs sr ON sr.id = rl.step_run_id \
         JOIN job_runs jr ON jr.id = sr.job_run_id \
         WHERE jr.workflow_run_id = ? AND rl.id > ? ORDER BY rl.id ASC",
    )
    .bind(workflow_run_id)
    .bind(since_id)
    .fetch_all(pool)
    .await
}

pub async fn list_logs_for_step(pool: &SqlitePool, step_run_id: &str, since_id: i64) -> sqlx::Result<Vec<RunLog>> {
    sqlx::query_as::<_, RunLog>("SELECT * FROM run_logs WHERE step_run_id = ? AND id > ? ORDER BY id ASC")
        .bind(step_run_id)
        .bind(since_id)
        .fetch_all(pool)
        .await
}

pub async fn run_tree(pool: &SqlitePool, workflow_run_id: &str) -> sqlx::Result<Option<RunTree>> {
    let Some(run) = find_run(pool, workflow_run_id).await? else {
        return Ok(None);
    };
    let jobs = list_job_runs(pool, workflow_run_id).await?;
    let mut job_trees = Vec::with_capacity(jobs.len());
    for job in jobs {
        let steps = list_step_runs(pool, &job.id).await?;
        job_trees.push(JobRunTree { job, steps });
    }
    Ok(Some(RunTree { run, jobs: job_trees }))
}
