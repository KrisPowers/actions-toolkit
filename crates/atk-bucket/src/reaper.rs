//! Janga: TTL-based auto-cleanup and startup crash reconciliation, widened from job shards
//! alone to also cover shells (the OS subprocesses that drive a workflow run's job DAG) and
//! bucket-scoped resource-cache build leases. Still one coherent sweep, not three separate
//! services: `reconcile_on_startup` handles what a crashed prior process left open,
//! `run_periodic_sweep` handles what normal completion hasn't caught up to yet.

use std::path::Path;
use std::sync::Arc;

use atk_db::queries::{buckets as bucket_queries, shards as shard_queries, resource_cache as cache_queries, shells as shell_queries};
use sqlx::SqlitePool;

const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
/// A resource-cache build lease with no heartbeat in this long is presumed abandoned (its builder
/// shell died mid-build) and reset so a waiting shell can retry instead of polling forever.
const STALE_BUILD_SECONDS: i64 = 120;

/// Startup-only pass: force-clean every job sandbox, shell, and bucket row still open when the
/// process starts, since anything still open at startup can only mean the previous process died
/// before it could tear itself down (there is no other path that leaves a row open).
pub async fn reconcile_on_startup(pool: &SqlitePool, buckets_root: &Path) {
    reconcile_sandboxes(pool, buckets_root).await;
    reconcile_shells(pool).await;
    reap_completed_buckets(pool).await;
}

async fn reconcile_sandboxes(pool: &SqlitePool, buckets_root: &Path) {
    let rows = match shard_queries::list_unreaped(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "failed to list unreaped job shards at startup");
            return;
        }
    };

    for row in rows {
        tracing::warn!(shard_id = %row.id, "cleaning up job sandbox left over from a previous process");
        let handle = super::handle_from_shard_row(buckets_root, &row);
        if let Err(e) = super::remove_shard(&handle).await {
            tracing::warn!(error = %e, shard_id = %row.id, "failed to force-clean a leftover job sandbox");
        }
        if let Err(e) = shard_queries::mark_reaped(pool, &row.id).await {
            tracing::warn!(error = %e, shard_id = %row.id, "failed to mark a leftover job sandbox reaped");
        }
    }
}

/// Any `shells` row with no `finished_at` at startup means its process (local: the shell itself;
/// remote: also the shell, once agents exist) died before it could report its own exit. Force-
/// kills a local shell by its stored PID (best-effort: a PID can in principle have been reused by
/// an unrelated process since, a known, accepted limitation of PID-based cleanup rather than one
/// this sweep tries to fully close) and marks it exited with a sentinel failure code either way,
/// so its bucket can still notice every shell is accounted for and complete.
async fn reconcile_shells(pool: &SqlitePool) {
    let rows = match shell_queries::list_unfinished(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "failed to list unfinished shells at startup");
            return;
        }
    };

    for row in rows {
        tracing::warn!(shell_id = %row.id, pid = ?row.pid, "cleaning up shell left over from a previous process");
        if row.agent_id.is_none() {
            if let Some(pid) = row.pid {
                if let Err(e) = kill_process(pid) {
                    tracing::warn!(error = %e, shell_id = %row.id, pid, "failed to force-kill a leftover shell process (it may have already exited)");
                }
            }
        }
        // Sentinel failure code (128, matching the "died from a signal"/"never completed"
        // convention rather than the exit-code range a real step failure uses) — this shell never
        // got the chance to report anything more specific.
        if let Err(e) = shell_queries::mark_exited(pool, &row.id, 128).await {
            tracing::warn!(error = %e, shell_id = %row.id, "failed to mark a leftover shell exited");
        }
    }
}

async fn reap_completed_buckets(pool: &SqlitePool) {
    let rows = match bucket_queries::list_unreaped(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "failed to list unreaped buckets at startup");
            return;
        }
    };
    for row in rows {
        // A bucket with no `completed_at` at startup had shells still running when the process
        // died; `reconcile_shells` above just force-marked all of those exited, so re-check
        // completion now rather than waiting for the next periodic sweep.
        if row.completed_at.is_none() {
            match shell_queries::list_unfinished_for_bucket(pool, &row.id).await {
                Ok(unfinished) if unfinished.is_empty() => {
                    let _ = bucket_queries::mark_completed(pool, &row.id).await;
                }
                Ok(_) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, bucket_id = %row.id, "failed to check whether a leftover bucket's shells are all finished");
                    continue;
                }
            }
        }
        if let Err(e) = bucket_queries::mark_reaped(pool, &row.id).await {
            tracing::warn!(error = %e, bucket_id = %row.id, "failed to mark a leftover bucket reaped");
        }
    }
}

/// Periodic sweep: force-clean any job sandbox whose TTL has passed but that normal job
/// completion hasn't reaped yet (e.g. a step wedged past its expected runtime), reap buckets whose
/// shells have all finished, and reset resource-cache build leases whose builder went silent.
pub async fn run_periodic_sweep(pool: SqlitePool, buckets_root: Arc<Path>) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    loop {
        interval.tick().await;

        let rows = match shard_queries::list_expired(&pool).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(error = %e, "failed to list expired job shards");
                Vec::new()
            }
        };
        for row in rows {
            tracing::warn!(shard_id = %row.id, "job sandbox exceeded its TTL, force-cleaning");
            let handle = super::handle_from_shard_row(&buckets_root, &row);
            if let Err(e) = super::remove_shard(&handle).await {
                tracing::warn!(error = %e, shard_id = %row.id, "failed to force-clean an expired job sandbox");
            }
            if let Err(e) = shard_queries::mark_reaped(&pool, &row.id).await {
                tracing::warn!(error = %e, shard_id = %row.id, "failed to mark an expired job sandbox reaped");
            }
        }

        match cache_queries::reap_stale_builds(&pool, STALE_BUILD_SECONDS).await {
            Ok(count) if count > 0 => tracing::warn!(count, "reset stale resource-cache build leases with no recent heartbeat"),
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "failed to sweep stale resource-cache build leases"),
        }

        let completed = match bucket_queries::list_completed_unreaped(&pool).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(error = %e, "failed to list completed, unreaped buckets");
                Vec::new()
            }
        };
        for row in completed {
            if let Err(e) = bucket_queries::mark_reaped(&pool, &row.id).await {
                tracing::warn!(error = %e, bucket_id = %row.id, "failed to mark a completed bucket reaped");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn kill_process(pid: i64) -> anyhow::Result<()> {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGKILL)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn kill_process(pid: i64) -> anyhow::Result<()> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid as u32)?;
        let result = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        result?;
    }
    Ok(())
}
