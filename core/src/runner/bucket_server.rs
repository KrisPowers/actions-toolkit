//! The control-plane side of a bucket: owns the one `LocalRunClient` for this bucket (and
//! therefore the DB pool, the durable encryption key's plaintext exposure, and the bucket's
//! ephemeral session key), and serves every shell spawned inside this bucket over RCP. A shell
//! never gets any of that directly, only request/response round-trips through here.

use std::sync::Arc;

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use atk_db::queries::{buckets as bucket_queries, shells as shell_queries};

use crate::runner::log_stream::LogHub;
use crate::runner::rcp_protocol::{ArtifactInfo, Hello, RcpRequest, RcpResponse};
use crate::runner::run_client::{LocalRunClient, RunClient};
use crate::runner::stats_hub::StatsHub;

/// Binds this bucket's RCP listeners (a local named-pipe/Unix-socket endpoint for same-machine
/// shells, plus a TCP listener for shells scheduled onto a remote agent — see the security note
/// on `atk_rcp::tcp` about that transport's current lack of TLS), starts serving both as a
/// background task, and returns the bound TCP address so the caller can hand it to a remote
/// agent. Binding happens before this function returns (not inside the background task) so a
/// caller never races a shell trying to connect before the listener actually exists.
#[allow(clippy::too_many_arguments)]
pub async fn spawn(
    db: SqlitePool,
    log_hub: Arc<LogHub>,
    stats_hub: Arc<StatsHub>,
    bucket_id: String,
    repo_id: String,
    auth_token_hash: String,
    local_endpoint: String,
    durable_enc: atk_crypto::EncryptionKey,
) -> Result<std::net::SocketAddr> {
    let run_client = Arc::new(
        LocalRunClient::new(db.clone(), log_hub, stats_hub, bucket_id.clone(), &repo_id, &durable_enc)
            .await
            .context("failed to build this bucket's local run client")?,
    );

    let local_listener =
        atk_rcp::LocalListener::bind(&local_endpoint).with_context(|| format!("failed to bind bucket RCP endpoint {local_endpoint}"))?;
    let tcp_listener = atk_rcp::tcp::TcpListener::bind(None).await.context("failed to bind bucket TCP RCP listener")?;
    let tcp_addr = tcp_listener.local_addr()?;

    tokio::spawn(serve_local(local_listener, run_client.clone(), db.clone(), bucket_id.clone(), auth_token_hash.clone()));
    tokio::spawn(serve_tcp(tcp_listener, run_client, db, bucket_id, auth_token_hash));

    Ok(tcp_addr)
}

async fn serve_local(mut listener: atk_rcp::LocalListener, run_client: Arc<LocalRunClient>, db: SqlitePool, bucket_id: String, auth_token_hash: String) {
    loop {
        let stream = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, bucket_id, "failed accepting a local RCP connection");
                return;
            }
        };
        spawn_connection_handler(stream, run_client.clone(), db.clone(), bucket_id.clone(), auth_token_hash.clone());
    }
}

async fn serve_tcp(mut listener: atk_rcp::tcp::TcpListener, run_client: Arc<LocalRunClient>, db: SqlitePool, bucket_id: String, auth_token_hash: String) {
    loop {
        let stream = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, bucket_id, "failed accepting a TCP RCP connection");
                return;
            }
        };
        spawn_connection_handler(stream, run_client.clone(), db.clone(), bucket_id.clone(), auth_token_hash.clone());
    }
}

fn spawn_connection_handler<S>(stream: S, run_client: Arc<LocalRunClient>, db: SqlitePool, bucket_id: String, auth_token_hash: String)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = handle_connection(stream, run_client, db, &bucket_id, &auth_token_hash).await {
            tracing::warn!(error = %e, bucket_id, "RCP connection ended with an error");
        }
    });
}

async fn handle_connection<S>(mut stream: S, run_client: Arc<LocalRunClient>, db: SqlitePool, bucket_id: &str, auth_token_hash: &str) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let hello: Hello = atk_rcp::framing::recv(&mut stream).await?.context("connection closed before sending Hello")?;
    if hello.bucket_id != bucket_id || !atk_auth::password::verify(&hello.auth_token, auth_token_hash) {
        atk_rcp::framing::send(&mut stream, &RcpResponse::Error("invalid bucket id or auth token".to_string())).await?;
        anyhow::bail!("rejected an RCP connection with a mismatched bucket id or auth token");
    }
    atk_rcp::framing::send(&mut stream, &RcpResponse::Ok).await?;

    loop {
        let Some(request) = atk_rcp::framing::recv::<_, RcpRequest>(&mut stream).await? else { return Ok(()) };
        let response = dispatch(&request, run_client.as_ref(), &db, bucket_id).await;
        atk_rcp::framing::send(&mut stream, &response).await?;
    }
}

async fn dispatch(request: &RcpRequest, run_client: &dyn RunClient, db: &SqlitePool, bucket_id: &str) -> RcpResponse {
    let result = handle(request, run_client, db, bucket_id).await;
    result.unwrap_or_else(|e| RcpResponse::Error(format!("{e:#}")))
}

async fn handle(request: &RcpRequest, run_client: &dyn RunClient, db: &SqlitePool, bucket_id: &str) -> Result<RcpResponse> {
    Ok(match request {
        RcpRequest::SetRunStatus { workflow_run_id, status, terminal } => {
            run_client.set_run_status(workflow_run_id, status, *terminal).await?;
            RcpResponse::Ok
        }
        RcpRequest::SetJobStatus { job_run_id, status, exit_code, terminal } => {
            run_client.set_job_status(job_run_id, status, *exit_code, *terminal).await?;
            RcpResponse::Ok
        }
        RcpRequest::SetJobContainer { job_run_id, container_id } => {
            run_client.set_job_container(job_run_id, container_id).await?;
            RcpResponse::Ok
        }
        RcpRequest::CreateStepRun { job_run_id, step_index, name, kind } => {
            let id = run_client.create_step_run(job_run_id, *step_index, name.as_deref(), kind).await?;
            RcpResponse::StepRunId(id)
        }
        RcpRequest::SetStepStatus { step_run_id, status, exit_code, terminal } => {
            run_client.set_step_status(step_run_id, status, *exit_code, *terminal).await?;
            RcpResponse::Ok
        }
        RcpRequest::InsertLogLine { step_run_id, ts, stream, message } => {
            run_client.insert_log_line(step_run_id, ts, stream, message).await?;
            RcpResponse::Ok
        }
        RcpRequest::FindArtifactByRunAndName { workflow_run_id, name } => {
            let path = run_client.find_artifact_by_run_and_name(workflow_run_id, name).await?;
            RcpResponse::Artifact(path.map(|path_on_disk| ArtifactInfo { path_on_disk }))
        }
        RcpRequest::RecordArtifact { workflow_run_id, job_run_id, name, path_on_disk, size_bytes } => {
            run_client.record_artifact(workflow_run_id, job_run_id.as_deref(), name, path_on_disk, *size_bytes).await?;
            RcpResponse::Ok
        }
        RcpRequest::ListSecretNames => RcpResponse::SecretNames(run_client.list_secret_names().await?),
        RcpRequest::RequestSecret { name } => RcpResponse::Secret(run_client.request_secret(name).await?),
        RcpRequest::ResourceCacheLookup { cache_key } => RcpResponse::ResourceCache(run_client.resource_cache_lookup(cache_key).await?),
        RcpRequest::ResourceCacheBeginBuild { cache_key, shell_id } => {
            RcpResponse::ResourceCache(run_client.resource_cache_begin_build(cache_key, shell_id).await?)
        }
        RcpRequest::ResourceCacheHeartbeat { entry_id } => {
            run_client.resource_cache_heartbeat(entry_id).await?;
            RcpResponse::Ok
        }
        RcpRequest::ResourceCacheComplete { entry_id, path_on_disk, size_bytes } => {
            run_client.resource_cache_complete(entry_id, path_on_disk, *size_bytes).await?;
            RcpResponse::Ok
        }
        RcpRequest::ResourceCacheFail { entry_id } => {
            run_client.resource_cache_fail(entry_id).await?;
            RcpResponse::Ok
        }
        RcpRequest::RecordJobShard { id, job_run_id, workflow_run_id, workspace_path, network_enabled, ttl_expires_at } => {
            run_client.record_job_shard(id, job_run_id, workflow_run_id, workspace_path, *network_enabled, ttl_expires_at).await?;
            RcpResponse::Ok
        }
        RcpRequest::MarkShardReaped { shard_id } => {
            run_client.mark_shard_reaped(shard_id).await?;
            RcpResponse::Ok
        }
        // Shell-lifecycle, not job-data, so it goes straight to the DB rather than through
        // `RunClient`: Janga's cleanup path (see `reaper.rs`) waits specifically on
        // `outcome_persisted_at`, which `mark_exited` only sets once every job/step status update
        // above has already round-tripped and returned `Ok` to the shell that awaited them.
        RcpRequest::ReportShellExit { shell_id, exit_code, cache_hits, cache_misses } => {
            shell_queries::record_cache_counters(db, shell_id, *cache_hits, *cache_misses).await?;
            shell_queries::mark_exited(db, shell_id, *exit_code).await?;
            // Once every shell this bucket ever spawned has reported its outcome, the bucket
            // itself is done: Janga's sweep (see `reaper.rs`) picks up `completed_at IS NOT NULL
            // AND reaped_at IS NULL` buckets and tears down what's left (the resource-cache
            // directory, this RCP server's listener).
            if shell_queries::list_unfinished_for_bucket(db, bucket_id).await?.is_empty() {
                bucket_queries::mark_completed(db, bucket_id).await?;
            }
            RcpResponse::Ok
        }
        RcpRequest::ReportResourceSample {
            subject_type,
            subject_id,
            workflow_run_id,
            ts,
            cpu_percent,
            memory_bytes,
            disk_read_bytes,
            disk_write_bytes,
            process_count,
            host_cpu_percent,
            host_memory_percent,
        } => {
            run_client
                .report_resource_sample(
                    subject_type,
                    subject_id,
                    workflow_run_id.as_deref(),
                    ts,
                    *cpu_percent,
                    *memory_bytes,
                    *disk_read_bytes,
                    *disk_write_bytes,
                    *process_count,
                    *host_cpu_percent,
                    *host_memory_percent,
                )
                .await?;
            RcpResponse::Ok
        }
    })
}
