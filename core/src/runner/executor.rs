use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use bollard::Docker;

use crate::bucket;
use crate::db::models::now_iso;
use crate::runner::run_client::RunClient;
use crate::runner::{artifact_capture, docker as docker_ops, sampler, workspace};
use crate::workflow::model::Job;

/// Resource-cache hit/miss counts a shell accumulates locally across every job in its DAG
/// (`run_inner` owns one `Arc<CacheCounters>` per shell run, cloned into each job's `run_job`
/// call), read back once at shell exit and reported alongside the exit code (see
/// `run_client::report_shell_exit`) — the only durable record of these counts.
#[derive(Default)]
pub struct CacheCounters {
    hits: AtomicI64,
    misses: AtomicI64,
}

impl CacheCounters {
    pub fn snapshot(&self) -> (i64, i64) {
        (self.hits.load(Ordering::Relaxed), self.misses.load(Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckoutContext {
    pub owner: String,
    pub repo: String,
    pub repo_id: String,
    pub pat: String,
    pub git_ref: String,
}

/// Which backend is actually running this job's `run:` steps: Docker exec when the job declares
/// a `container:`, or the native Shard otherwise. Decided once up front so the step loop
/// and artifact capture don't need to re-derive it. `uses: docker://` steps are unaffected by
/// this, they always get their own one-off container regardless of which backend the job uses.
enum RunBackend {
    Docker { docker: Docker, container_id: String },
    Shard { handle: bucket::ShardHandle },
}

/// Execute a single job: checkout (if configured), start its container or sandbox, run each step
/// in order, capture declared artifacts, and always clean up afterward. Returns `true` if every
/// step succeeded. Every database touch goes through `run_client` — this function runs inside a
/// shell subprocess and has no direct database access of its own, only whatever `run_client`
/// (an `RcpRunClient` in real use) round-trips to the owning bucket for.
#[allow(clippy::too_many_arguments)]
pub async fn run_job(
    run_client: &Arc<dyn RunClient>,
    docker: &Option<Docker>,
    workspaces_dir: &Path,
    buckets_dir: &Path,
    artifacts_dir: &Path,
    bucket_id: &str,
    shell_id: &str,
    workflow_run_id: &str,
    job_run_id: &str,
    job: &Job,
    checkout: Option<CheckoutContext>,
    cache_counters: &Arc<CacheCounters>,
) -> Result<bool> {
    run_client.set_job_status(job_run_id, "running", None, false).await?;

    // Keyed by job_run_id, not workflow_run_id: each job gets its own workspace so files one job
    // writes aren't implicitly visible to jobs that run after it. download_artifacts is the only
    // way to pass files between jobs, matching GitHub Actions' own per-job isolation.
    let workspace_dir = workspace::ensure(workspaces_dir, job_run_id)?;

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

    // Repo-scoped secrets: only their names cross into this process from the bucket up front;
    // each value is decrypted on the bucket's side, under its ephemeral session key, and handed
    // over only at this point-of-use request, never held or logged here beyond this point.
    if checkout.is_some() {
        match run_client.list_secret_names().await {
            Ok(names) => {
                for name in names {
                    match run_client.request_secret(&name).await {
                        Ok(Some(value)) => injected_env.push(format!("{name}={value}")),
                        Ok(None) => {}
                        Err(e) => {
                            emit_system_line(run_client, job_run_id, &format!("failed to request secret '{name}': {e}")).await;
                        }
                    }
                }
            }
            Err(e) => {
                emit_system_line(run_client, job_run_id, &format!("failed to look up secrets for this repo: {e}")).await;
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
            emit_system_line(run_client, job_run_id, &format!("checkout failed: {e}")).await;
            run_client.set_job_status(job_run_id, "failed", Some(-1), true).await?;
            return Ok(false);
        }
    }

    for name in &job.download_artifacts {
        match run_client.find_artifact_by_run_and_name(workflow_run_id, name).await {
            Ok(Some(path_on_disk)) => {
                let dest = workspace_dir.join(name);
                if let Err(e) = copy_recursive(Path::new(&path_on_disk), &dest) {
                    emit_system_line(run_client, job_run_id, &format!("failed to stage artifact '{name}': {e}")).await;
                }
            }
            Ok(None) => {
                emit_system_line(run_client, job_run_id, &format!("declared download_artifacts entry '{name}' was not found on this run")).await;
            }
            Err(e) => {
                emit_system_line(run_client, job_run_id, &format!("failed to look up artifact '{name}': {e}")).await;
            }
        }
    }

    let backend = match &job.container {
        Some(container_spec) => {
            let Some(docker) = docker else {
                emit_system_line(run_client, job_run_id, "job declares a container: but Docker is not available on this host").await;
                run_client.set_job_status(job_run_id, "failed", Some(-1), true).await?;
                return Ok(false);
            };

            let env: Vec<String> = container_spec
                .env
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| format!("{k}={v}")).collect())
                .unwrap_or_default();

            if let Err(e) = docker_ops::pull_image(docker, &container_spec.image).await {
                emit_system_line(run_client, job_run_id, &format!("failed to pull image '{}': {e}", container_spec.image)).await;
                run_client.set_job_status(job_run_id, "failed", Some(-1), true).await?;
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
                    emit_system_line(run_client, job_run_id, &format!("failed to start job container: {e}")).await;
                    run_client.set_job_status(job_run_id, "failed", Some(-1), true).await?;
                    return Ok(false);
                }
            };
            run_client.set_job_container(job_run_id, &container_id).await?;
            RunBackend::Docker { docker: docker.clone(), container_id }
        }
        None => {
            // Bucket-level TTL/host-mount settings aren't resolved here anymore (that lived on
            // `Settings`, which a shell has no direct access to) — `bucket::DEFAULT_TTL` and no
            // extra mounts is the same conservative default a settings lookup failure already
            // fell back to before, and per-bucket TTL/mount overrides are a follow-up once that
            // configuration has an RCP-reachable home.
            let spec = bucket::ShardSpec {
                workspace_host_path: &workspace_dir,
                network_enabled: job.network,
                ttl: bucket::DEFAULT_TTL,
                extra_ro_mounts: &[],
            };
            match bucket::create_job_shard(buckets_dir, spec).await {
                Ok(handle) => {
                    let ttl_expires_at = (chrono::Utc::now() + chrono::Duration::seconds(bucket::DEFAULT_TTL.as_secs() as i64)).to_rfc3339();
                    if let Err(e) = run_client
                        .record_job_shard(&handle.id, job_run_id, workflow_run_id, &handle.workspace.to_string_lossy(), job.network, &ttl_expires_at)
                        .await
                    {
                        tracing::warn!(error = %e, shard_id = %handle.id, "failed to record job sandbox bookkeeping row");
                    }
                    RunBackend::Shard { handle }
                }
                Err(e) => {
                    emit_system_line(run_client, job_run_id, &format!("failed to create sandbox: {e}")).await;
                    run_client.set_job_status(job_run_id, "failed", Some(-1), true).await?;
                    return Ok(false);
                }
            }
        }
    };

    // Only the native Shard backend has anything of ours to sample directly (a job container's
    // resource usage isn't attributed here at all yet — Docker stats is a separate follow-up, see
    // the observability plan's scoping note); aborted right before the shard itself is torn down
    // below, so the sampler never outlives what it's sampling.
    let shard_sampler = match &backend {
        RunBackend::Shard { handle } => Some(sampler::spawn_shard_sampler(run_client.clone(), handle.clone(), workflow_run_id.to_string())),
        RunBackend::Docker { .. } => None,
    };

    let mut job_succeeded = true;
    // (entry_id, cache_key, workspace-relative path) for every cache step that came back a miss
    // and won this shell the build lease. Saved into the bucket-scoped cache once, after the job
    // succeeds, so a save only ever captures a fully-populated directory, never a partial one from
    // a job that went on to fail.
    let mut pending_cache_saves: Vec<(String, String, String)> = Vec::new();

    for (index, step) in job.steps.iter().enumerate() {
        let step_run_id = run_client.create_step_run(job_run_id, index as i64, step.name.as_deref(), step.kind()).await?;

        if !job_succeeded && !step.continue_on_error {
            run_client.set_step_status(&step_run_id, "skipped", None, true).await?;
            continue;
        }

        run_client.set_step_status(&step_run_id, "running", None, false).await?;

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

        let exit_code = if step.kind() == "cache" {
            handle_cache_step(run_client, bucket_id, shell_id, buckets_dir, &workspace_dir, step, &mut pending_cache_saves, cache_counters).await
        } else if let Some(command) = &step.run {
            let run_client_for_lines = run_client.clone();
            let step_run_id_for_lines = step_run_id.clone();
            let on_line = move |stream: &str, message: String| {
                let run_client = run_client_for_lines.clone();
                let step_run_id = step_run_id_for_lines.clone();
                let stream = stream.to_string();
                tokio::spawn(async move {
                    let _ = run_client.insert_log_line(&step_run_id, &now_iso(), &stream, &message).await;
                });
            };

            let result: Result<i64> = match &backend {
                RunBackend::Docker { docker, container_id } => {
                    docker_ops::exec_step(docker, container_id, command, step.shell.as_deref(), None, &step_env, on_line)
                        .await
                        .map(|r| r.exit_code)
                }
                RunBackend::Shard { handle } => {
                    bucket::exec_step(handle, command, step.shell.as_deref(), None, &step_env, on_line).await.map(|r| r.exit_code)
                }
            };
            match result {
                Ok(exit_code) => exit_code,
                Err(e) => {
                    emit_system_line(run_client, job_run_id, &format!("step '{:?}' failed: {e}", step.name)).await;
                    -1
                }
            }
        } else if let Some(uses) = &step.uses {
            if let Some(image) = uses.strip_prefix("docker://") {
                exec_docker_action_step(
                    run_client,
                    docker,
                    image,
                    uses,
                    step.name.as_deref(),
                    &step_run_id,
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
        run_client.set_step_status(&step_run_id, step_status, Some(exit_code), true).await?;

        if exit_code != 0 && !step.continue_on_error {
            job_succeeded = false;
        }
    }

    if job_succeeded {
        for (entry_id, cache_key, relative_path) in &pending_cache_saves {
            let src = workspace_dir.join(relative_path);
            let dest = cache_root_for_bucket(buckets_dir, bucket_id).join(sanitize_cache_key(cache_key));
            let _ = std::fs::remove_dir_all(&dest);
            let save_result = copy_recursive(&src, &dest).and_then(|()| dir_size(&dest));
            match save_result {
                Ok(size_bytes) => {
                    if let Err(e) = run_client.resource_cache_complete(entry_id, &dest.to_string_lossy(), size_bytes as i64).await {
                        tracing::warn!(error = %e, cache_key, "failed to record a completed resource-cache build");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, cache_key, "failed to save a resource-cache build, marking it failed so a waiter can retry");
                    if let Err(e) = run_client.resource_cache_fail(entry_id).await {
                        tracing::warn!(error = %e, cache_key, "failed to mark a resource-cache build failed");
                    }
                }
            }
        }
    }

    if job_succeeded && !job.artifacts.is_empty() {
        let capture_result = match &backend {
            RunBackend::Docker { docker, container_id } => {
                artifact_capture::capture(docker, run_client, container_id, artifacts_dir, workflow_run_id, job_run_id, &job.artifacts).await
            }
            RunBackend::Shard { .. } => {
                artifact_capture::capture_from_workspace(run_client, &workspace_dir, artifacts_dir, workflow_run_id, job_run_id, &job.artifacts)
                    .await
            }
        };
        if let Err(e) = capture_result {
            emit_system_line(run_client, job_run_id, &format!("artifact capture failed: {e}")).await;
        }
    }

    if let Some(sampler) = &shard_sampler {
        sampler.abort();
    }

    match &backend {
        RunBackend::Docker { docker, container_id } => {
            if let Err(e) = docker_ops::remove_container(docker, container_id).await {
                tracing::warn!(error = %e, container_id, "failed to remove job container");
            }
        }
        RunBackend::Shard { handle } => {
            if let Err(e) = bucket::remove_shard(handle).await {
                tracing::warn!(error = %e, shard_id = %handle.id, "failed to remove job sandbox");
            }
            if let Err(e) = run_client.mark_shard_reaped(&handle.id).await {
                tracing::warn!(error = %e, shard_id = %handle.id, "failed to mark job sandbox reaped");
            }
        }
    }

    let final_status = if job_succeeded { "succeeded" } else { "failed" };
    run_client.set_job_status(job_run_id, final_status, Some(if job_succeeded { 0 } else { 1 }), true).await?;

    Ok(job_succeeded)
}

/// Runs a `uses: docker://image` step's one-off container action. Always goes through Docker
/// regardless of whether the job itself is Docker- or Shard-backed, since a container action is
/// its own self-contained container, not tied to the job's `run:` step backend.
#[allow(clippy::too_many_arguments)]
async fn exec_docker_action_step(
    run_client: &Arc<dyn RunClient>,
    docker: &Option<Docker>,
    image: &str,
    uses: &str,
    step_name: Option<&str>,
    step_run_id: &str,
    workspace_dir: &Path,
    workflow_run_id: &str,
    job_run_id: &str,
    step_env: &[String],
) -> i64 {
    let Some(docker) = docker else {
        emit_system_line(
            run_client,
            job_run_id,
            &format!("step '{step_name:?}' uses a docker:// action but Docker is not available on this host"),
        )
        .await;
        return -1;
    };

    let run_client_for_lines = run_client.clone();
    let step_run_id_owned = step_run_id.to_string();
    let result = docker_ops::run_container_action(
        docker,
        image,
        workspace_dir,
        workflow_run_id,
        job_run_id,
        step_env,
        |stream, message| {
            let run_client = run_client_for_lines.clone();
            let step_run_id = step_run_id_owned.clone();
            let stream = stream.to_string();
            tokio::spawn(async move {
                let _ = run_client.insert_log_line(&step_run_id, &now_iso(), &stream, &message).await;
            });
        },
    )
    .await;

    match result {
        Ok(r) => r.exit_code,
        Err(e) => {
            emit_system_line(run_client, job_run_id, &format!("container action '{uses}' failed: {e}")).await;
            -1
        }
    }
}

async fn emit_system_line(_run_client: &Arc<dyn RunClient>, job_run_id: &str, message: &str) {
    // System-level messages (image pull failure, checkout failure) aren't tied to a specific
    // step_run row, so they're logged via tracing only; per-step failures are captured
    // through the normal exec_step/run_container_action streaming path above.
    tracing::warn!(job_run_id, message);
}

/// Handles a `uses: cache` step: `with: { key, path }`, optionally `cross_platform: true` to
/// share the entry across OSes/architectures (the default keys every entry to this host's OS and
/// arch, so e.g. a Windows and a macOS shell in the same bucket never accidentally reuse each
/// other's native-binary output). On a hit, restores a read-only copy into this job's own
/// workspace (never a shared mutable mount, preserving per-job workspace isolation). On a miss,
/// either claims the build lease (recorded in `pending_cache_saves` for the post-job save once
/// this job succeeds) or polls briefly for a concurrent builder to finish. Always best-effort: a
/// cache problem degrades to "the next steps just regenerate the resource fresh," never fails the
/// step outright.
async fn handle_cache_step(
    run_client: &Arc<dyn RunClient>,
    bucket_id: &str,
    shell_id: &str,
    buckets_dir: &Path,
    workspace_dir: &Path,
    step: &crate::workflow::model::Step,
    pending_cache_saves: &mut Vec<(String, String, String)>,
    cache_counters: &CacheCounters,
) -> i64 {
    let Some(with) = &step.with else {
        tracing::warn!("cache step is missing a `with:` block, skipping");
        return 0;
    };
    let Some(key_template) = with.get("key").and_then(|v| v.as_str()) else {
        tracing::warn!("cache step is missing `with.key`, skipping");
        return 0;
    };
    let Some(relative_path) = with.get("path").and_then(|v| v.as_str()) else {
        tracing::warn!("cache step is missing `with.path`, skipping");
        return 0;
    };
    let cross_platform = with.get("cross_platform").and_then(|v| v.as_bool()).unwrap_or(false);

    let cache_key = resolve_cache_key(key_template, workspace_dir, cross_platform);
    let cache_root = cache_root_for_bucket(buckets_dir, bucket_id);

    match run_client.resource_cache_lookup(&cache_key).await {
        Ok(state) if state.status == "ready" => {
            cache_counters.hits.fetch_add(1, Ordering::Relaxed);
            if let Some(path_on_disk) = &state.path_on_disk {
                if let Err(e) = copy_recursive(Path::new(path_on_disk), &workspace_dir.join(relative_path)) {
                    tracing::warn!(error = %e, cache_key, "failed to restore a ready resource-cache entry");
                }
            }
            return 0;
        }
        Ok(_) => {
            cache_counters.misses.fetch_add(1, Ordering::Relaxed);
        }
        Err(e) => {
            cache_counters.misses.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(error = %e, cache_key, "resource cache lookup failed, treating as a miss");
        }
    }

    match run_client.resource_cache_begin_build(&cache_key, shell_id).await {
        Ok(state) if state.is_builder => {
            pending_cache_saves.push((state.entry_id, cache_key, relative_path.to_string()));
        }
        Ok(state) => {
            // Someone else is already building it: poll with backoff for a bounded time rather
            // than forever, since a builder that never heartbeats gets reset by the periodic reap
            // sweep (see `atk_bucket::reaper`), not by this loop.
            for attempt in 0..10u32 {
                tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1).min(5))).await;
                match run_client.resource_cache_lookup(&cache_key).await {
                    Ok(s) if s.status == "ready" => {
                        if let Some(path_on_disk) = &s.path_on_disk {
                            if let Err(e) = copy_recursive(Path::new(path_on_disk), &workspace_dir.join(relative_path)) {
                                tracing::warn!(error = %e, cache_key, "failed to restore a resource-cache entry after waiting for its builder");
                            }
                        }
                        break;
                    }
                    Ok(s) if s.status == "failed" || s.status == "miss" => break,
                    _ => continue,
                }
            }
            let _ = (cache_root, state);
        }
        Err(e) => {
            tracing::warn!(error = %e, cache_key, "failed to claim a resource-cache build lease, skipping caching for this step");
        }
    }

    0
}

/// Substitutes every `${{ hashFiles('glob') }}` occurrence in `template` with a short hex digest
/// of the matched files' contents (sorted by path first, so the hash doesn't depend on filesystem
/// iteration order), leaving the rest of the template as a literal. Deliberately not routed
/// through the full `${{ }}` expression evaluator (which only handles `if:` conditions today) —
/// this is the one `hashFiles`-shaped expression a cache key needs, not a general expression.
fn resolve_cache_key(template: &str, workspace_dir: &Path, cross_platform: bool) -> String {
    let mut resolved = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${{") {
        resolved.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("}}") else {
            resolved.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let expr = rest[start + 3..start + end].trim();
        if let Some(inner) = expr.strip_prefix("hashFiles(").and_then(|s| s.strip_suffix(')')) {
            let pattern = inner.trim().trim_matches(|c| c == '\'' || c == '"');
            resolved.push_str(&hash_files(workspace_dir, pattern));
        }
        rest = &rest[start + end + 2..];
    }
    resolved.push_str(rest);

    if cross_platform {
        resolved
    } else {
        format!("{resolved}-{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Hashes every file directly under `workspace_dir` matching `glob_pattern` (a bare filename or
/// `*`-glob, not a full glob-syntax path pattern — the common `hashFiles('package-lock.json')` /
/// `hashFiles('**/*.lock')`-style single-segment cases), sorted by filename for determinism.
fn hash_files(workspace_dir: &Path, glob_pattern: &str) -> String {
    use sha2::{Digest, Sha256};
    let file_name_pattern = glob_pattern.rsplit('/').next().unwrap_or(glob_pattern);
    let mut matched: Vec<std::path::PathBuf> = std::fs::read_dir(workspace_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            globset::Glob::new(file_name_pattern).map(|g| g.compile_matcher().is_match(name)).unwrap_or(false)
        })
        .collect();
    matched.sort();

    let mut hasher = Sha256::new();
    for path in matched {
        if let Ok(bytes) = std::fs::read(&path) {
            hasher.update(&bytes);
        }
    }
    hex::encode(hasher.finalize())[..16].to_string()
}

fn sanitize_cache_key(cache_key: &str) -> String {
    cache_key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn cache_root_for_bucket(buckets_dir: &Path, bucket_id: &str) -> std::path::PathBuf {
    buckets_dir.parent().unwrap_or(buckets_dir).join("bucket-cache").join(bucket_id)
}

fn dir_size(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if dir.is_file() {
        return Ok(std::fs::metadata(dir)?.len());
    }
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += dir_size(&path)?;
        } else if let Ok(meta) = std::fs::metadata(&path) {
            total += meta.len();
        }
    }
    Ok(total)
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
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
    use uuid::Uuid;

    use super::*;
    use crate::config::AppConfig;
    use crate::crypto::EncryptionKey;
    use crate::runner::log_stream::LogHub;
    use crate::runner::run_client::LocalRunClient;
    use crate::runner::stats_hub::StatsHub;
    use crate::workflow::model::{ArtifactSpec, Job, Step};

    async fn test_run_client(db: sqlx::SqlitePool, repo_id: &str) -> Arc<dyn RunClient> {
        let enc = EncryptionKey::generate_ephemeral(); // stands in for the durable key in tests
        let log_hub = Arc::new(LogHub::new());
        let stats_hub = Arc::new(StatsHub::new());
        let client = LocalRunClient::new(db, log_hub, stats_hub, "test-bucket".to_string(), repo_id, &enc)
            .await
            .expect("LocalRunClient::new should succeed");
        Arc::new(client)
    }

    /// End-to-end: a job with no `container:` should run its `run:` step via the Shard backend
    /// (not Docker, which is `None` here on purpose) and produce a declared artifact, exercising
    /// the actual `RunBackend::Shard` path through a real `LocalRunClient`, the same code path
    /// a shell's `RcpRunClient` calls drive on the other side of the wire in real use.
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
        seed_fk_chain(&db, "repo-1", "workflow-1", "run-1", "job-1").await;
        let run_client = test_run_client(db, "repo-1").await;

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
                // executor.rs's RunBackend::Shard wiring (shell resolution itself is covered
                // separately in bucket::windows::tests), and cmd.exe's simpler inherited-cwd
                // handling sidesteps a known, separate AppContainer/PowerShell working-directory
                // gap for deeply nested workspace paths (tracked as a follow-up).
                shell: if cfg!(windows) { Some("cmd".to_string()) } else { None },
            }],
            artifacts: vec![ArtifactSpec { name: "out".to_string(), path: format!("/workspace/{out_file}") }],
            download_artifacts: vec![],
            network: false,
        };

        let succeeded = run_job(
            &run_client,
            &None,
            &config.workspaces_dir(),
            &config.buckets_dir(),
            &config.artifacts_dir(),
            "test-bucket",
            "test-shell",
            "run-1",
            "job-1",
            &job,
            None,
            &Arc::new(CacheCounters::default()),
        )
        .await
        .expect("run_job should not error");
        assert!(succeeded, "expected the job to succeed running via the sandbox");

        // `capture_from_workspace` mirrors the Docker path's convention: `dest_dir` (here
        // `artifacts_dir/run-1/out`) *is* the artifact's destination, whether the source was a
        // single file (as here) or a directory, not a container directory named after it.
        let artifact_path = config.artifacts_dir().join("run-1").join("out");
        assert!(artifact_path.exists(), "expected the artifact to have been captured to {}", artifact_path.display());
        assert_eq!(std::fs::read_to_string(&artifact_path).unwrap().trim(), "hello");

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// Rule-proving test for the workspace-isolation fix: two jobs in the same run must not see
    /// each other's files unless explicitly passed via `download_artifacts`. job-a writes a file
    /// into its own workspace; job-b actively checks (the same way a real step would, with a
    /// live conditional, not just a path comparison from the test itself) whether that file is
    /// visible to it and records the result as its own artifact.
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

        seed_fk_chain(&db, "repo-2", "workflow-2", "run-2", "job-a").await;
        sqlx::query("INSERT INTO job_runs (id, workflow_run_id, job_key, status) VALUES ('job-b', 'run-2', 'second', 'running')")
            .execute(&db)
            .await
            .unwrap();
        let run_client = test_run_client(db, "repo-2").await;

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

        let job_a_ok = run_job(
            &run_client,
            &None,
            &config.workspaces_dir(),
            &config.buckets_dir(),
            &config.artifacts_dir(),
            "test-bucket",
            "test-shell",
            "run-2",
            "job-a",
            &job_a,
            None,
            &Arc::new(CacheCounters::default()),
        )
        .await
        .expect("job-a should not error");
        assert!(job_a_ok, "expected job-a to succeed");
        let job_b_ok = run_job(
            &run_client,
            &None,
            &config.workspaces_dir(),
            &config.buckets_dir(),
            &config.artifacts_dir(),
            "test-bucket",
            "test-shell",
            "run-2",
            "job-b",
            &job_b,
            None,
            &Arc::new(CacheCounters::default()),
        )
        .await
        .expect("job-b should not error");
        assert!(job_b_ok, "expected job-b to succeed");

        let marker_path = config.artifacts_dir().join("run-2").join("marker");
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
