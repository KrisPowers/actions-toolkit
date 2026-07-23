//! The `__shell-run` entry point: what actually executes inside a shell subprocess. Reads a spec
//! file the control plane wrote, connects back to its owning bucket over RCP, and drives the
//! workflow run's job DAG (`scheduler::run_inner`) using that connection as its only database
//! access — this process never opens the SQLite file itself. Unlike `__sandbox-init`, this does
//! not need to run before the tokio runtime is built: spawning this subprocess is a plain
//! `posix_spawn`/`CreateProcess` call, not a `fork()`, so it's safe to trigger from inside an
//! already-running multi-threaded runtime.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::runner::executor::CheckoutContext;
use crate::runner::run_client::{report_shell_exit, RcpRunClient, RunClient};
use crate::runner::scheduler;
use crate::workflow::model::Workflow;

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellRunSpec {
    pub bucket_id: String,
    pub shell_id: String,
    pub rcp_endpoint: String,
    pub auth_token: String,
    pub workflow_run_id: String,
    pub workflow: Workflow,
    pub job_runs: Vec<(String, String)>,
    pub checkout: Option<CheckoutContext>,
    pub trigger_event: String,
    pub max_concurrent_jobs: usize,
    pub workspaces_dir: PathBuf,
    pub buckets_dir: PathBuf,
    pub artifacts_dir: PathBuf,
}

pub async fn run(spec_path: PathBuf) -> Result<i32> {
    let spec_bytes = std::fs::read(&spec_path).context("failed to read shell run spec")?;
    let spec: ShellRunSpec = serde_json::from_slice(&spec_bytes).context("failed to parse shell run spec")?;
    let _ = std::fs::remove_file(&spec_path);

    let capability = atk_bucket::probe_capability().await;
    if !capability.ok {
        anyhow::bail!(
            "this host cannot run job sandboxes ({}); a shell has nowhere to run jobs without one",
            capability.reason.as_deref().unwrap_or("unknown reason")
        );
    }

    let stream = atk_rcp::connect(&spec.rcp_endpoint).await.context("failed to connect to this shell's owning bucket")?;
    let rcp_client =
        Arc::new(RcpRunClient::handshake(stream, &spec.bucket_id, &spec.auth_token).await.context("RCP handshake with the owning bucket failed")?);
    let run_client: Arc<dyn RunClient> = rcp_client.clone();

    let docker = crate::runner::docker::connect(None).ok();

    let result = scheduler::run_inner(
        &run_client,
        &docker,
        &spec.workspaces_dir,
        &spec.buckets_dir,
        &spec.artifacts_dir,
        &spec.bucket_id,
        &spec.shell_id,
        &spec.workflow_run_id,
        &spec.workflow,
        spec.checkout,
        &spec.trigger_event,
        spec.max_concurrent_jobs,
        spec.job_runs,
    )
    .await;

    let exit_code = match &result {
        Ok(succeeded) => {
            if *succeeded {
                0
            } else {
                1
            }
        }
        Err(e) => {
            tracing::error!(error = %e, workflow_run_id = %spec.workflow_run_id, "shell's job DAG run failed");
            1
        }
    };

    // The one place a shell tells its bucket it's completely done; the bucket only stamps
    // `outcome_persisted_at` after this call, and every status update the DAG run just performed
    // above already round-tripped and returned before we get here, so the outcome really is
    // durable by the time this arrives.
    if let Err(e) = report_shell_exit(rcp_client.as_ref(), &spec.shell_id, exit_code).await {
        tracing::warn!(error = %e, shell_id = %spec.shell_id, "failed to report this shell's exit to its bucket");
    }

    Ok(exit_code as i32)
}
