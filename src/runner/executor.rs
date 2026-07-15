use anyhow::Result;
use bollard::Docker;

use crate::app::AppState;
use crate::db::models::now_iso;
use crate::db::queries::{artifacts as artifact_queries, runs as run_queries};
use crate::runner::log_stream::LogLine;
use crate::runner::{artifact_capture, docker as docker_ops, workspace};
use crate::workflow::model::Job;

pub struct CheckoutContext {
    pub owner: String,
    pub repo: String,
    pub pat: String,
    pub git_ref: String,
}

/// Execute a single job: checkout (if configured), start its container, run each step in
/// order, capture declared artifacts, and always clean up the container. Returns `true` if
/// every step succeeded.
pub async fn run_job(
    state: &AppState,
    docker: &Docker,
    workflow_run_id: &str,
    job_run_id: &str,
    job: &Job,
    checkout: Option<CheckoutContext>,
) -> Result<bool> {
    run_queries::set_job_status(&state.db, job_run_id, "running", None, false).await?;

    let workspace_dir = workspace::ensure(&state.config.workspaces_dir(), workflow_run_id)?;

    if let Some(ctx) = &checkout {
        let owner = ctx.owner.clone();
        let repo = ctx.repo.clone();
        let pat = ctx.pat.clone();
        let git_ref = ctx.git_ref.clone();
        let dir = workspace_dir.clone();
        let checkout_result =
            tokio::task::spawn_blocking(move || crate::github::checkout::checkout(&owner, &repo, &pat, &git_ref, &dir))
                .await?;
        if let Err(e) = checkout_result {
            emit_system_line(state, job_run_id, &format!("checkout failed: {e}")).await;
            run_queries::set_job_status(&state.db, job_run_id, "failed", Some(-1), true).await?;
            return Ok(false);
        }
    }

    for name in &job.download_artifacts {
        match artifact_queries::find_by_run_and_name(&state.db, workflow_run_id, name).await {
            Ok(Some(artifact)) => {
                let dest = workspace_dir.join(name);
                if let Err(e) = copy_recursive(std::path::Path::new(&artifact.path_on_disk), &dest) {
                    emit_system_line(state, job_run_id, &format!("failed to stage artifact '{name}': {e}")).await;
                }
            }
            Ok(None) => {
                emit_system_line(state, job_run_id, &format!("declared download_artifacts entry '{name}' was not found on this run")).await;
            }
            Err(e) => {
                emit_system_line(state, job_run_id, &format!("failed to look up artifact '{name}': {e}")).await;
            }
        }
    }

    let env: Vec<String> = job
        .container
        .env
        .as_ref()
        .map(|m| m.iter().map(|(k, v)| format!("{k}={v}")).collect())
        .unwrap_or_default();

    if let Err(e) = docker_ops::pull_image(docker, &job.container.image).await {
        emit_system_line(state, job_run_id, &format!("failed to pull image '{}': {e}", job.container.image)).await;
        run_queries::set_job_status(&state.db, job_run_id, "failed", Some(-1), true).await?;
        return Ok(false);
    }

    let container_id = match docker_ops::create_job_container(
        docker,
        &job.container.image,
        &workspace_dir,
        workflow_run_id,
        job_run_id,
        &env,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            emit_system_line(state, job_run_id, &format!("failed to start job container: {e}")).await;
            run_queries::set_job_status(&state.db, job_run_id, "failed", Some(-1), true).await?;
            return Ok(false);
        }
    };
    run_queries::set_job_container(&state.db, job_run_id, &container_id).await?;

    let mut job_succeeded = true;

    for (index, step) in job.steps.iter().enumerate() {
        let step_run = run_queries::create_step_run(
            &state.db,
            job_run_id,
            index as i64,
            step.name.as_deref(),
            step.kind(),
        )
        .await?;

        if !job_succeeded && !step.continue_on_error {
            run_queries::set_step_status(&state.db, &step_run.id, "skipped", None, true).await?;
            continue;
        }

        run_queries::set_step_status(&state.db, &step_run.id, "running", None, false).await?;

        let step_env: Vec<String> = step
            .env
            .as_ref()
            .map(|m| m.iter().map(|(k, v)| format!("{k}={v}")).collect())
            .unwrap_or_default();

        let exit_code = if let Some(command) = &step.run {
            let hub = state.log_hub.clone();
            let pool = state.db.clone();
            let step_run_id = step_run.id.clone();
            let result = docker_ops::exec_step(docker, &container_id, command, None, &step_env, |stream, message| {
                let hub = hub.clone();
                let pool = pool.clone();
                let step_run_id = step_run_id.clone();
                let stream = stream.to_string();
                tokio::spawn(async move {
                    hub.publish(
                        &pool,
                        LogLine {
                            step_run_id,
                            ts: now_iso(),
                            stream,
                            message,
                        },
                    )
                    .await;
                });
            })
            .await;
            match result {
                Ok(r) => r.exit_code,
                Err(e) => {
                    emit_system_line(state, job_run_id, &format!("step '{:?}' failed: {e}", step.name)).await;
                    -1
                }
            }
        } else if let Some(uses) = &step.uses {
            if let Some(image) = uses.strip_prefix("docker://") {
                let hub = state.log_hub.clone();
                let pool = state.db.clone();
                let step_run_id = step_run.id.clone();
                let result = docker_ops::run_container_action(
                    docker,
                    image,
                    &workspace_dir,
                    workflow_run_id,
                    job_run_id,
                    &step_env,
                    |stream, message| {
                        let hub = hub.clone();
                        let pool = pool.clone();
                        let step_run_id = step_run_id.clone();
                        let stream = stream.to_string();
                        tokio::spawn(async move {
                            hub.publish(
                                &pool,
                                LogLine {
                                    step_run_id,
                                    ts: now_iso(),
                                    stream,
                                    message,
                                },
                            )
                            .await;
                        });
                    },
                )
                .await;
                match result {
                    Ok(r) => r.exit_code,
                    Err(e) => {
                        emit_system_line(state, job_run_id, &format!("container action '{uses}' failed: {e}")).await;
                        -1
                    }
                }
            } else {
                // "uses: checkout" and any other non-docker `uses` are no-ops beyond the
                // job-level checkout already performed above.
                0
            }
        } else {
            0
        };

        let step_status = if exit_code == 0 { "succeeded" } else { "failed" };
        run_queries::set_step_status(&state.db, &step_run.id, step_status, Some(exit_code), true).await?;
        state.log_hub.close(&step_run.id);

        if exit_code != 0 && !step.continue_on_error {
            job_succeeded = false;
        }
    }

    if job_succeeded && !job.artifacts.is_empty() {
        if let Err(e) = artifact_capture::capture(
            docker,
            &state.db,
            &container_id,
            &state.config.artifacts_dir(),
            workflow_run_id,
            job_run_id,
            &job.artifacts,
        )
        .await
        {
            emit_system_line(state, job_run_id, &format!("artifact capture failed: {e}")).await;
        }
    }

    if let Err(e) = docker_ops::remove_container(docker, &container_id).await {
        tracing::warn!(error = %e, container_id, "failed to remove job container");
    }

    let final_status = if job_succeeded { "succeeded" } else { "failed" };
    run_queries::set_job_status(&state.db, job_run_id, final_status, Some(if job_succeeded { 0 } else { 1 }), true)
        .await?;

    Ok(job_succeeded)
}

async fn emit_system_line(_state: &AppState, job_run_id: &str, message: &str) {
    // System-level messages (image pull failure, checkout failure) aren't tied to a specific
    // step_run row, so they're logged via tracing only; per-step failures are captured
    // through the normal exec_step/run_container_action streaming path above.
    tracing::warn!(job_run_id, message);
}

fn copy_recursive(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest)?;
    }
    Ok(())
}
