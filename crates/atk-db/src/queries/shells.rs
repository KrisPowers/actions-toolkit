use sqlx::SqlitePool;

use crate::models::{now_iso, Shell};

/// `id` is caller-supplied (not generated here) so a caller can embed it in a `ShellRunSpec`
/// serialized into `spec_json` before this row exists — the spec needs to carry its own shell id,
/// and generating the id here would make that a chicken-and-egg problem for the agent-assigned
/// path. `initial_status` is `"running"` for a shell the control plane spawns itself right away
/// (the local, no-agent-needed path), or `"assigned"` for one scheduled onto a remote agent that
/// hasn't picked it up yet (see `mark_started`, which transitions `assigned` -> `running` once
/// the agent actually spawns the process).
#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    id: &str,
    bucket_id: &str,
    workflow_run_id: &str,
    target_os: &str,
    agent_id: Option<&str>,
    initial_status: &str,
    spec_json: Option<&str>,
) -> sqlx::Result<Shell> {
    let now = now_iso();
    sqlx::query(
        "INSERT INTO shells (id, bucket_id, workflow_run_id, target_os, agent_id, status, spec_json, started_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(bucket_id)
    .bind(workflow_run_id)
    .bind(target_os)
    .bind(agent_id)
    .bind(initial_status)
    .bind(spec_json)
    .bind(&now)
    .execute(pool)
    .await?;

    find(pool, id).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Transitions an agent-assigned shell to `running` once the agent has actually spawned the
/// process and reports its PID (meaningful only on the agent's own host, but recorded here for
/// visibility/diagnostics either way).
pub async fn mark_started(pool: &SqlitePool, id: &str, pid: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE shells SET status = 'running', pid = ? WHERE id = ?").bind(pid).bind(id).execute(pool).await?;
    Ok(())
}

/// Shells assigned to a given agent that it hasn't started yet, the agent's poll target.
pub async fn list_assigned_for_agent(pool: &SqlitePool, agent_id: &str) -> sqlx::Result<Vec<Shell>> {
    sqlx::query_as::<_, Shell>("SELECT * FROM shells WHERE agent_id = ? AND status = 'assigned'").bind(agent_id).fetch_all(pool).await
}

pub async fn find(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<Shell>> {
    sqlx::query_as::<_, Shell>("SELECT * FROM shells WHERE id = ?").bind(id).fetch_optional(pool).await
}

pub async fn set_pid(pool: &SqlitePool, id: &str, pid: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE shells SET pid = ? WHERE id = ?").bind(pid).bind(id).execute(pool).await?;
    Ok(())
}

/// Marks the shell exited and, once every `job_runs`/`step_runs` row it owns is already terminal
/// (guaranteed by the caller: the shell only reports exit after awaiting those status updates),
/// stamps `outcome_persisted_at` too. Janga's cleanup path waits for this column specifically,
/// not just `finished_at`, so a shell's process boundary is never torn down before its outcome is
/// durably recorded.
pub async fn mark_exited(pool: &SqlitePool, id: &str, exit_code: i64) -> sqlx::Result<()> {
    let now = now_iso();
    sqlx::query("UPDATE shells SET status = 'exited', exit_code = ?, finished_at = ?, outcome_persisted_at = ? WHERE id = ?")
        .bind(exit_code)
        .bind(&now)
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_reaped(pool: &SqlitePool, id: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE shells SET reaped_at = ? WHERE id = ?").bind(now_iso()).bind(id).execute(pool).await?;
    Ok(())
}

/// Records the resource-cache hit/miss counts a shell accumulated locally while running its job
/// DAG (see `core::runner::executor::handle_cache_step`), reported once alongside its exit code.
pub async fn record_cache_counters(pool: &SqlitePool, id: &str, cache_hits: i64, cache_misses: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE shells SET cache_hits = ?, cache_misses = ? WHERE id = ?")
        .bind(cache_hits)
        .bind(cache_misses)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Shells still open (no `finished_at`) belonging to a bucket, used to decide whether a bucket is
/// safe to tear down yet (only once this list is empty).
pub async fn list_unfinished_for_bucket(pool: &SqlitePool, bucket_id: &str) -> sqlx::Result<Vec<Shell>> {
    sqlx::query_as::<_, Shell>("SELECT * FROM shells WHERE bucket_id = ? AND finished_at IS NULL")
        .bind(bucket_id)
        .fetch_all(pool)
        .await
}

/// Shells whose outcome is durably persisted but that haven't been reaped yet, Janga's
/// completion-triggered cleanup target.
pub async fn list_ready_to_reap(pool: &SqlitePool) -> sqlx::Result<Vec<Shell>> {
    sqlx::query_as::<_, Shell>("SELECT * FROM shells WHERE outcome_persisted_at IS NOT NULL AND reaped_at IS NULL")
        .fetch_all(pool)
        .await
}

/// Every still-open shell (no `finished_at`) regardless of bucket, used once at startup to find
/// shells whose owning process (control plane or, for a local shell, the shell itself) crashed
/// before it could report its own exit.
pub async fn list_unfinished(pool: &SqlitePool) -> sqlx::Result<Vec<Shell>> {
    sqlx::query_as::<_, Shell>("SELECT * FROM shells WHERE finished_at IS NULL").fetch_all(pool).await
}
