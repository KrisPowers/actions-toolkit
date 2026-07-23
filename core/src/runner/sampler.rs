//! Periodic resource sampling for a running shell and the shards it drives. Each sampler is a
//! background task that reports through `RunClient::report_resource_sample` (never touching the
//! database directly, same rule as everything else a shell does) until its `AbortHandle` is
//! aborted by the caller. Nothing here is billing-grade: cpu percentages are derived from a fixed
//! polling interval, not a precisely-timed accounting window, which is the right tradeoff for
//! "give the operator a good picture of this run" insights rather than metering.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sysinfo::{Pid, System};
use tokio::task::AbortHandle;

use crate::db::models::now_iso;
use crate::runner::run_client::RunClient;

const SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// Every live descendant of `root` (`root` included) as of this `System` snapshot: the shell
/// itself plus whatever it has spawned (shard-init launchers, step subprocesses, Docker CLI
/// helpers), found by repeatedly expanding the known-PID set until a pass adds nothing new.
fn process_tree_pids(sys: &System, root: Pid) -> HashSet<Pid> {
    let mut tree = HashSet::new();
    tree.insert(root);
    loop {
        let mut grew = false;
        for (pid, process) in sys.processes() {
            if tree.contains(pid) {
                continue;
            }
            if process.parent().is_some_and(|parent| tree.contains(&parent)) {
                tree.insert(*pid);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    tree
}

struct TreeSample {
    cpu_percent: f64,
    memory_bytes: i64,
    disk_read_bytes: i64,
    disk_write_bytes: i64,
    process_count: i64,
}

fn sample_tree(sys: &System, root: Pid) -> TreeSample {
    let mut sample = TreeSample { cpu_percent: 0.0, memory_bytes: 0, disk_read_bytes: 0, disk_write_bytes: 0, process_count: 0 };
    for pid in process_tree_pids(sys, root) {
        let Some(process) = sys.process(pid) else { continue };
        sample.cpu_percent += process.cpu_usage() as f64;
        sample.memory_bytes += process.memory() as i64;
        let disk = process.disk_usage();
        sample.disk_read_bytes += disk.read_bytes as i64;
        sample.disk_write_bytes += disk.written_bytes as i64;
        sample.process_count += 1;
    }
    sample
}

/// Spawns the periodic sampler for a whole shell's process tree, reporting `subject_type =
/// "shell"` samples until aborted. The caller (`shell_run::run`) starts this right after the RCP
/// handshake and aborts it just before reporting the shell's own exit, so the sampled tree is
/// always still-live work, never a snapshot taken after everything has already exited.
pub fn spawn_shell_sampler(run_client: Arc<dyn RunClient>, shell_id: String, workflow_run_id: String) -> AbortHandle {
    let task = tokio::spawn(async move {
        let mut sys = System::new_all();
        let root = Pid::from_u32(std::process::id());
        loop {
            tokio::time::sleep(SAMPLE_INTERVAL).await;
            sys.refresh_all();
            let sample = sample_tree(&sys, root);
            let host_cpu_percent = sys.global_cpu_usage() as f64;
            let host_memory_percent =
                if sys.total_memory() > 0 { sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0 } else { 0.0 };

            if let Err(e) = run_client
                .report_resource_sample(
                    "shell",
                    &shell_id,
                    Some(&workflow_run_id),
                    &now_iso(),
                    Some(sample.cpu_percent),
                    Some(sample.memory_bytes),
                    Some(sample.disk_read_bytes),
                    Some(sample.disk_write_bytes),
                    Some(sample.process_count),
                    Some(host_cpu_percent),
                    Some(host_memory_percent),
                )
                .await
            {
                tracing::debug!(error = %e, shell_id, "failed to report a shell-level resource sample");
            }
        }
    });
    task.abort_handle()
}

/// Spawns the periodic sampler for one shard's cgroup accounting (Linux) via
/// `atk_bucket::read_shard_accounting`. On Windows that function always returns an all-`None`
/// reading (see its doc comment), so this loop simply never reports a sample there; the Backend
/// tab falls back to showing that shard's parent shell's own numbers for its active window
/// instead of a broken zeroed-out card. Reports `subject_type = "shard"` samples until aborted;
/// the caller (`executor::run_job`) starts this once the shard's cgroup exists and aborts it right
/// before tearing the shard down.
pub fn spawn_shard_sampler(run_client: Arc<dyn RunClient>, handle: atk_bucket::ShardHandle, workflow_run_id: String) -> AbortHandle {
    let task = tokio::spawn(async move {
        let mut prev_cpu_usage_usec: Option<u64> = None;
        loop {
            tokio::time::sleep(SAMPLE_INTERVAL).await;
            let accounting = atk_bucket::read_shard_accounting(&handle);
            if accounting.memory_bytes.is_none() && accounting.cpu_usage_usec.is_none() && accounting.process_count.is_none() {
                // Nothing readable on this platform/host for this shard; don't write a
                // misleadingly empty row every tick.
                continue;
            }

            let cpu_percent = match (prev_cpu_usage_usec, accounting.cpu_usage_usec) {
                (Some(prev), Some(now)) if now >= prev => {
                    Some((now - prev) as f64 / SAMPLE_INTERVAL.as_micros() as f64 * 100.0)
                }
                _ => None,
            };
            prev_cpu_usage_usec = accounting.cpu_usage_usec;

            if let Err(e) = run_client
                .report_resource_sample(
                    "shard",
                    &handle.id,
                    Some(&workflow_run_id),
                    &now_iso(),
                    cpu_percent,
                    accounting.memory_bytes.map(|v| v as i64),
                    None,
                    None,
                    accounting.process_count.map(|v| v as i64),
                    None,
                    None,
                )
                .await
            {
                tracing::debug!(error = %e, shard_id = %handle.id, "failed to report a shard-level resource sample");
            }
        }
    });
    task.abort_handle()
}
