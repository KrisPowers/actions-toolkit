use anyhow::Result;
use bollard::Docker;

use crate::app::AppState;
use crate::bucket;
use crate::db::models::now_iso;
use crate::db::queries::{artifacts as artifact_queries, buckets as bucket_queries, runs as run_queries, secrets as secret_queries};
use crate::runner::log_stream::LogLine;
use crate::runner::{artifact_capture, docker as docker_ops, workspace};
use crate::workflow::model::Job;

pub struct CheckoutContext {
    pub owner: String,
    pub repo: String,
    pub repo_id: String,
    pub pat: String,
    pub git_ref: String,
}

/// Which backend is actually running this job's `run:` steps: Docker exec when the job declares
/// a `container:`, or the native Bucket sandbox otherwise. Decided once up front so the step loop
/// and artifact capture don't need to re-derive it. `uses: docker://` steps are unaffected by
/// this — they always get their own one-off container regardless of which backend the job uses.
enum RunBackend {
    Docker { docker: Docker, container_id: String },
    Bucket { handle: bucket::BucketHandle },
}

/// Execute a single job: checkout (if configured), start its container or sandbox, run each step
/// in order, capture declared artifacts, and always clean up afterward. Returns `true` if every
/// step succeeded.
pub async fn run_job(
    state: &AppState,
    docker: &Option<Docker>,
    workflow_run_id: &str,
    job_run_id: &str,
    job: &Job,
    checkout: Option<CheckoutContext>,
) -> Result<bool> {
    run_queries::set_job_status(&state.db, job_run_id, "running", None, false).await?;

    // Keyed by job_run_id, not workflow_run_id: each job gets its own workspace so files one job
    // writes aren't implicitly visible to jobs that run after it. download_artifacts is the only
    // way to pass files between jobs, matching GitHub Actions' own per-job isolation.
    let workspace_dir = workspace::ensure(&state.config.workspaces_dir(), job_run_id)?;

    // Mirrors GitHub Actions' automatic `GITHUB_TOKEN`: steps need the same credential checkout
    // already uses to reach the GitHub API themselves (e.g. to update a release).
    let mut injected_env: Vec<String> = checkout
        .as_ref()
        .map(|ctx| {
            vec![
                format!("GITHUB_TOKEN={}", ctx.pat),
                format!("GITHUB_REPOSITORY={}/{}", ctx.owner, ctx.repo),
            ]
        })
        .unwrap_or_default();

    // Repo-scoped secrets, decrypted just-in-time and injected the same way: never written back
    // to disk in plaintext, never logged (see the docs on the step_env merge below), only ever
    // present as an env var inside the running job's own container/sandbox process.
    if let Some(ctx) = &checkout {
        match secret_queries::list_for_repo(&state.db, &ctx.repo_id).await {
            Ok(secrets) => {
                for secret in secrets {
                    match state.enc.decrypt_str(&secret.value_encrypted, &secret.value_nonce) {
                        Ok(value) => injected_env.push(format!("{}={value}", secret.name)),
                        Err(e) => {
                            emit_system_line(state, job_run_id, &format!("failed to decrypt secret '{}': {e}", secret.name)).await;
                        }
                    }
                }
            }
            Err(e) => {
                emit_system_line(state, job_run_id, &format!("failed to look up secrets for this repo: {e}")).await;
            }
        }
    }

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

    let backend = match &job.container {
        Some(container_spec) => {
            let Some(docker) = docker else {
                emit_system_line(state, job_run_id, "job declares a container: but Docker is not available on this host").await;
                run_queries::set_job_status(&state.db, job_run_id, "failed", Some(-1), true).await?;
                return Ok(false);
            };

            let env: Vec<String> = container_spec
                .env
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| format!("{k}={v}")).collect())
                .unwrap_or_default();

            if let Err(e) = docker_ops::pull_image(docker, &container_spec.image).await {
                emit_system_line(state, job_run_id, &format!("failed to pull image '{}': {e}", container_spec.image)).await;
                run_queries::set_job_status(&state.db, job_run_id, "failed", Some(-1), true).await?;
                return Ok(false);
            }

            let container_id = match docker_ops::create_job_container(
                docker,
                &container_spec.image,
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
            RunBackend::Docker { docker: docker.clone(), container_id }
        }
        None => {
            let spec = bucket::BucketSpec {
                workspace_host_path: &workspace_dir,
                run_id: workflow_run_id,
                job_run_id,
                network_enabled: job.network,
                ttl: bucket::DEFAULT_TTL,
            };
            match bucket::create_job_bucket(&state.db, &state.config.buckets_dir(), spec).await {
                Ok(handle) => RunBackend::Bucket { handle },
                Err(e) => {
                    emit_system_line(state, job_run_id, &format!("failed to create sandbox: {e}")).await;
                    run_queries::set_job_status(&state.db, job_run_id, "failed", Some(-1), true).await?;
                    return Ok(false);
                }
            }
        }
    };

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

        let mut step_env: Vec<String> = step
            .env
            .as_ref()
            .map(|m| m.iter().map(|(k, v)| format!("{k}={v}")).collect())
            .unwrap_or_default();
        let declared_keys: std::collections::HashSet<String> =
            step_env.iter().filter_map(|e| e.split('=').next().map(str::to_string)).collect();
        step_env.extend(
            injected_env
                .iter()
                .filter(|e| !declared_keys.contains(e.split('=').next().unwrap_or_default()))
                .cloned(),
        );

        let exit_code = if let Some(command) = &step.run {
            let hub = state.log_hub.clone();
            let pool = state.db.clone();
            let step_run_id = step_run.id.clone();
            let on_line = move |stream: &str, message: String| {
                let hub = hub.clone();
                let pool = pool.clone();
                let step_run_id = step_run_id.clone();
                let stream = stream.to_string();
                tokio::spawn(async move {
                    hub.publish(&pool, LogLine { step_run_id, ts: now_iso(), stream, message }).await;
                });
            };

            let result: Result<i64> = match &backend {
                RunBackend::Docker { docker, container_id } => {
                    docker_ops::exec_step(docker, container_id, command, step.shell.as_deref(), None, &step_env, on_line)
                        .await
                        .map(|r| r.exit_code)
                }
                RunBackend::Bucket { handle } => {
                    bucket::exec_step(handle, command, step.shell.as_deref(), None, &step_env, on_line).await.map(|r| r.exit_code)
                }
            };
            match result {
                Ok(exit_code) => exit_code,
                Err(e) => {
                    emit_system_line(state, job_run_id, &format!("step '{:?}' failed: {e}", step.name)).await;
                    -1
                }
            }
        } else if let Some(uses) = &step.uses {
            if let Some(image) = uses.strip_prefix("docker://") {
                exec_docker_action_step(
                    state,
                    docker,
                    image,
                    uses,
                    step.name.as_deref(),
                    &step_run.id,
                    &workspace_dir,
                    workflow_run_id,
                    job_run_id,
                    &step_env,
                )
                .await
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
        let capture_result = match &backend {
            RunBackend::Docker { docker, container_id } => {
                artifact_capture::capture(
                    docker,
                    &state.db,
                    container_id,
                    &state.config.artifacts_dir(),
                    workflow_run_id,
                    job_run_id,
                    &job.artifacts,
                )
                .await
            }
            RunBackend::Bucket { .. } => {
                artifact_capture::capture_from_workspace(
                    &state.db,
                    &workspace_dir,
                    &state.config.artifacts_dir(),
                    workflow_run_id,
                    job_run_id,
                    &job.artifacts,
                )
                .await
            }
        };
        if let Err(e) = capture_result {
            emit_system_line(state, job_run_id, &format!("artifact capture failed: {e}")).await;
        }
    }

    match &backend {
        RunBackend::Docker { docker, container_id } => {
            if let Err(e) = docker_ops::remove_container(docker, container_id).await {
                tracing::warn!(error = %e, container_id, "failed to remove job container");
            }
        }
        RunBackend::Bucket { handle } => {
            if let Err(e) = bucket::remove_bucket(&state.db, handle).await {
                tracing::warn!(error = %e, bucket_id = %handle.id, "failed to remove bucket");
            }
            if let Err(e) = bucket_queries::mark_reaped(&state.db, &handle.id).await {
                tracing::warn!(error = %e, bucket_id = %handle.id, "failed to mark bucket reaped");
            }
        }
    }

    let final_status = if job_succeeded { "succeeded" } else { "failed" };
    run_queries::set_job_status(&state.db, job_run_id, final_status, Some(if job_succeeded { 0 } else { 1 }), true)
        .await?;

    Ok(job_succeeded)
}

/// Runs a `uses: docker://image` step's one-off container action. Always goes through Docker
/// regardless of whether the job itself is Docker- or Bucket-backed, since a container action is
/// its own self-contained container, not tied to the job's `run:` step backend.
#[allow(clippy::too_many_arguments)]
async fn exec_docker_action_step(
    state: &AppState,
    docker: &Option<Docker>,
    image: &str,
    uses: &str,
    step_name: Option<&str>,
    step_run_id: &str,
    workspace_dir: &std::path::Path,
    workflow_run_id: &str,
    job_run_id: &str,
    step_env: &[String],
) -> i64 {
    let Some(docker) = docker else {
        emit_system_line(
            state,
            job_run_id,
            &format!("step '{step_name:?}' uses a docker:// action but Docker is not available on this host"),
        )
        .await;
        return -1;
    };

    let hub = state.log_hub.clone();
    let pool = state.db.clone();
    let step_run_id = step_run_id.to_string();
    let result = docker_ops::run_container_action(
        docker,
        image,
        workspace_dir,
        workflow_run_id,
        job_run_id,
        step_env,
        |stream, message| {
            let hub = hub.clone();
            let pool = pool.clone();
            let step_run_id = step_run_id.clone();
            let stream = stream.to_string();
            tokio::spawn(async move {
                hub.publish(&pool, LogLine { step_run_id, ts: now_iso(), stream, message }).await;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;
    use uuid::Uuid;

    use super::*;
    use crate::app::AppStateInner;
    use crate::auth::jwt::JwtCodec;
    use crate::config::AppConfig;
    use crate::crypto::EncryptionKey;
    use crate::runner::log_stream::LogHub;
    use crate::workflow::model::{ArtifactSpec, Job, Step};

    /// End-to-end: a job with no `container:` should run its `run:` step via the Bucket sandbox
    /// (not Docker, which is `None` here on purpose) and produce a declared artifact, exercising
    /// the actual `RunBackend::Bucket` path added when Bucket was wired into the executor rather
    /// than just its individual pieces in isolation.
    #[tokio::test]
    async fn job_without_container_runs_via_bucket_and_captures_artifacts() {
        let capability = crate::bucket::probe_capability().await;
        if !capability.ok {
            eprintln!("skipping: host does not support Bucket ({:?})", capability.reason);
            return;
        }

        let test_id = Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-executor-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let config = AppConfig {
            data_dir: data_dir.clone(),
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: crate::github::oauth::GITHUB_TOKEN_URL.to_string(),
            github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
        };
        let db = crate::db::connect(&config.db_path()).await.expect("db connect should succeed");
        let enc = EncryptionKey::load_or_generate(None, &config.secrets_dir()).expect("encryption key should load");
        let jwt = JwtCodec::new("test-secret");

        seed_fk_chain(&db, "repo-1", "workflow-1", "run-1", "job-1").await;

        let state = AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt,
            enc,
            docker: None,
            bucket_capability_ok: true,
            log_hub: Arc::new(LogHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
        }));

        let out_file = "artifact.txt";
        let write_command = format!("echo hello > {out_file}");

        let job = Job {
            name: None,
            runs_on: "self-hosted".to_string(),
            container: None,
            needs: vec![],
            if_condition: None,
            strategy: None,
            steps: vec![Step {
                name: Some("write artifact".to_string()),
                id: None,
                run: Some(write_command),
                uses: None,
                with: None,
                env: None,
                if_condition: None,
                continue_on_error: false,
                // `cmd` rather than the pwsh/powershell default on Windows: this test exercises
                // executor.rs's RunBackend::Bucket wiring (shell resolution itself is covered
                // separately in bucket::windows::tests), and cmd.exe's simpler inherited-cwd
                // handling sidesteps a known, separate AppContainer/PowerShell working-directory
                // gap for deeply nested workspace paths (tracked as a follow-up).
                shell: if cfg!(windows) { Some("cmd".to_string()) } else { None },
            }],
            artifacts: vec![ArtifactSpec { name: "out".to_string(), path: format!("/workspace/{out_file}") }],
            download_artifacts: vec![],
            network: false,
        };

        let succeeded = run_job(&state, &None, "run-1", "job-1", &job, None).await.expect("run_job should not error");
        assert!(succeeded, "expected the job to succeed running via Bucket");

        // `capture_from_workspace` mirrors the Docker path's convention: `dest_dir` (here
        // `artifacts_dir/run-1/out`) *is* the artifact's destination, whether the source was a
        // single file (as here) or a directory, not a container directory named after it.
        let artifact_path = state.config.artifacts_dir().join("run-1").join("out");
        assert!(artifact_path.exists(), "expected the artifact to have been captured to {}", artifact_path.display());
        assert_eq!(std::fs::read_to_string(&artifact_path).unwrap().trim(), "hello");

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// Rule-proving test for the workspace-isolation fix: two jobs in the same run must not see
    /// each other's files unless explicitly passed via `download_artifacts`. job-a writes a file
    /// into its own workspace; job-b actively checks (the same way a real step would, with a
    /// live conditional, not just a path comparison from the test itself) whether that file is
    /// visible to it and records the result as its own artifact. Before the fix, both jobs
    /// resolved to the same workflow_run_id-keyed directory, so job-b would have seen job-a's
    /// file despite declaring no download_artifacts.
    #[tokio::test]
    async fn jobs_in_the_same_run_do_not_share_a_workspace() {
        let capability = crate::bucket::probe_capability().await;
        if !capability.ok {
            eprintln!("skipping: host does not support Bucket ({:?})", capability.reason);
            return;
        }

        let test_id = Uuid::new_v4().to_string();
        let data_dir = std::env::temp_dir().join(format!("atk-executor-isolation-test-{test_id}"));
        std::fs::create_dir_all(&data_dir).unwrap();

        let config = AppConfig {
            data_dir: data_dir.clone(),
            github_app_client_id: "test-client-id".to_string(),
            github_oauth_token_url: crate::github::oauth::GITHUB_TOKEN_URL.to_string(),
            github_device_code_url: crate::github::oauth::GITHUB_DEVICE_CODE_URL.to_string(),
        };
        let db = crate::db::connect(&config.db_path()).await.expect("db connect should succeed");
        let enc = EncryptionKey::load_or_generate(None, &config.secrets_dir()).expect("encryption key should load");
        let jwt = JwtCodec::new("test-secret");

        seed_fk_chain(&db, "repo-2", "workflow-2", "run-2", "job-a").await;
        sqlx::query("INSERT INTO job_runs (id, workflow_run_id, job_key, status) VALUES ('job-b', 'run-2', 'second', 'running')")
            .execute(&db)
            .await
            .unwrap();

        let state = AppState(Arc::new(AppStateInner {
            db,
            config,
            jwt,
            enc,
            docker: None,
            bucket_capability_ok: true,
            log_hub: Arc::new(LogHub::new()),
            github_client: RwLock::new(None),
            pending_device_flow: RwLock::new(None),
        }));

        let shell = if cfg!(windows) { Some("cmd".to_string()) } else { None };
        let write_command = "echo hello > only-in-job-a.txt".to_string();
        let check_command = if cfg!(windows) {
            "if exist only-in-job-a.txt (echo LEAKED > marker.txt) else (echo ISOLATED > marker.txt)".to_string()
        } else {
            "if [ -f only-in-job-a.txt ]; then echo LEAKED > marker.txt; else echo ISOLATED > marker.txt; fi".to_string()
        };

        let job_a = Job {
            name: None,
            runs_on: "self-hosted".to_string(),
            container: None,
            needs: vec![],
            if_condition: None,
            strategy: None,
            steps: vec![Step {
                name: Some("write a file into this job's own workspace".to_string()),
                id: None,
                run: Some(write_command),
                uses: None,
                with: None,
                env: None,
                if_condition: None,
                continue_on_error: false,
                shell: shell.clone(),
            }],
            artifacts: vec![],
            download_artifacts: vec![],
            network: false,
        };

        let job_b = Job {
            name: None,
            runs_on: "self-hosted".to_string(),
            container: None,
            needs: vec![],
            if_condition: None,
            strategy: None,
            steps: vec![Step {
                name: Some("check whether job-a's file leaked into this workspace".to_string()),
                id: None,
                run: Some(check_command),
                uses: None,
                with: None,
                env: None,
                if_condition: None,
                continue_on_error: false,
                shell,
            }],
            artifacts: vec![ArtifactSpec { name: "marker".to_string(), path: "/workspace/marker.txt".to_string() }],
            download_artifacts: vec![],
            network: false,
        };

        let job_a_ok = run_job(&state, &None, "run-2", "job-a", &job_a, None).await.expect("job-a should not error");
        assert!(job_a_ok, "expected job-a to succeed");
        let job_b_ok = run_job(&state, &None, "run-2", "job-b", &job_b, None).await.expect("job-b should not error");
        assert!(job_b_ok, "expected job-b to succeed");

        let marker_path = state.config.artifacts_dir().join("run-2").join("marker");
        assert!(marker_path.exists(), "expected job-b's marker artifact to have been captured to {}", marker_path.display());
        assert_eq!(
            std::fs::read_to_string(&marker_path).unwrap().trim(),
            "ISOLATED",
            "job-b must not see the file job-a wrote into its own workspace"
        );

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    async fn seed_fk_chain(pool: &sqlx::SqlitePool, repo_id: &str, workflow_id: &str, run_id: &str, job_run_id: &str) {
        let now = crate::db::models::now_iso();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, ?, 'admin', ?, ?)",
        )
        .bind("user-1")
        .bind("test-user")
        .bind("hash")
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO repos (id, owner, name, default_branch, webhook_secret_encrypted, \
             webhook_secret_nonce, created_by, created_at, updated_at) VALUES (?, 'test-owner', 'test-repo', 'main', \
             x'00', x'00', 'user-1', ?, ?)",
        )
        .bind(repo_id)
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
}
