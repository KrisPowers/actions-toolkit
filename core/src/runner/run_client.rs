//! `RunClient`: what `executor::run_job`/`scheduler::run_inner` use for every database touch,
//! instead of holding a `SqlitePool` directly. `LocalRunClient` is the real implementation,
//! instantiated once per bucket by `bucket_server` and never handed to a shell. `RcpRunClient` is
//! what a shell actually holds: every call serializes to an `RcpRequest` and crosses the wire to
//! whichever `LocalRunClient` owns that shell's bucket. This is what makes "a shell never touches
//! the database or holds a decryption key directly" true by construction rather than by
//! convention.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use atk_crypto::EncryptionKey;
use atk_db::queries::{
    artifacts as artifact_queries, shards as shard_queries, resource_cache as cache_queries,
    runs as run_queries, secrets as secret_queries,
};

use crate::runner::log_stream::{LogHub, LogLine};
use crate::runner::rcp_protocol::{Hello, RcpRequest, RcpResponse, ResourceCacheState};

#[async_trait]
pub trait RunClient: Send + Sync {
    async fn set_run_status(&self, workflow_run_id: &str, status: &str, terminal: bool) -> Result<()>;
    async fn set_job_status(&self, job_run_id: &str, status: &str, exit_code: Option<i64>, terminal: bool) -> Result<()>;
    async fn set_job_container(&self, job_run_id: &str, container_id: &str) -> Result<()>;
    async fn create_step_run(&self, job_run_id: &str, step_index: i64, name: Option<&str>, kind: &str) -> Result<String>;
    async fn set_step_status(&self, step_run_id: &str, status: &str, exit_code: Option<i64>, terminal: bool) -> Result<()>;
    async fn insert_log_line(&self, step_run_id: &str, ts: &str, stream: &str, message: &str) -> Result<()>;
    async fn find_artifact_by_run_and_name(&self, workflow_run_id: &str, name: &str) -> Result<Option<String>>;
    async fn record_artifact(
        &self,
        workflow_run_id: &str,
        job_run_id: Option<&str>,
        name: &str,
        path_on_disk: &str,
        size_bytes: i64,
    ) -> Result<()>;
    /// Names only, never values, same convention as the existing secrets API (`GET
    /// /repos/:id/secrets`) — lets a job know which env vars to populate without exposing any
    /// plaintext until `request_secret` is actually called for one.
    async fn list_secret_names(&self) -> Result<Vec<String>>;
    /// The point-of-use secret decrypt: returns the plaintext value for one repo-scoped secret,
    /// or `None` if no secret with that name exists for this bucket's repo. Never returns more
    /// than the one value asked for.
    async fn request_secret(&self, name: &str) -> Result<Option<String>>;
    async fn resource_cache_lookup(&self, cache_key: &str) -> Result<ResourceCacheState>;
    async fn resource_cache_begin_build(&self, cache_key: &str, shell_id: &str) -> Result<ResourceCacheState>;
    async fn resource_cache_heartbeat(&self, entry_id: &str) -> Result<()>;
    async fn resource_cache_complete(&self, entry_id: &str, path_on_disk: &str, size_bytes: i64) -> Result<()>;
    async fn resource_cache_fail(&self, entry_id: &str) -> Result<()>;
    /// Records a job sandbox's bookkeeping row. `atk_bucket::create_job_shard` itself does no
    /// database access at all (a shell has none), so this is the caller's job once it has a live
    /// `ShardHandle` back.
    async fn record_job_shard(&self, id: &str, job_run_id: &str, workflow_run_id: &str, workspace_path: &str, network_enabled: bool, ttl_expires_at: &str) -> Result<()>;
    async fn mark_shard_reaped(&self, shard_id: &str) -> Result<()>;
}

/// Secrets re-encrypted under the bucket's own ephemeral, never-persisted key immediately at
/// bucket startup, so the durable at-rest key's plaintext exposure window is just the moment of
/// startup, not the bucket's whole lifetime. `request_secret` decrypts one value from here per
/// call; the ephemeral key and every wrapped ciphertext are dropped when the bucket (and this
/// struct) is torn down.
struct EphemeralSecrets {
    key: EncryptionKey,
    wrapped: HashMap<String, (Vec<u8>, Vec<u8>)>,
}

pub struct LocalRunClient {
    db: SqlitePool,
    log_hub: Arc<LogHub>,
    bucket_id: String,
    secrets: EphemeralSecrets,
}

impl LocalRunClient {
    /// Decrypts every repo-scoped secret once under the durable at-rest key and immediately
    /// re-wraps it under a fresh ephemeral key, so this is the only point in a bucket's lifetime
    /// the durable key ever touches plaintext for these values.
    pub async fn new(db: SqlitePool, log_hub: Arc<LogHub>, bucket_id: String, repo_id: &str, durable_enc: &EncryptionKey) -> Result<Self> {
        let ephemeral = EncryptionKey::generate_ephemeral();
        let mut wrapped = HashMap::new();
        for secret in secret_queries::list_for_repo(&db, repo_id).await.context("failed to list secrets for bucket")? {
            let plaintext = durable_enc
                .decrypt_str(&secret.value_encrypted, &secret.value_nonce)
                .with_context(|| format!("failed to decrypt secret '{}' under the durable key", secret.name))?;
            let rewrapped = ephemeral.encrypt_str(&plaintext).context("failed to re-encrypt secret under the ephemeral key")?;
            wrapped.insert(secret.name, rewrapped);
        }
        Ok(Self { db, log_hub, bucket_id, secrets: EphemeralSecrets { key: ephemeral, wrapped } })
    }
}

#[async_trait]
impl RunClient for LocalRunClient {
    async fn set_run_status(&self, workflow_run_id: &str, status: &str, terminal: bool) -> Result<()> {
        run_queries::set_run_status(&self.db, workflow_run_id, status, terminal).await.map_err(Into::into)
    }

    async fn set_job_status(&self, job_run_id: &str, status: &str, exit_code: Option<i64>, terminal: bool) -> Result<()> {
        run_queries::set_job_status(&self.db, job_run_id, status, exit_code, terminal).await.map_err(Into::into)
    }

    async fn set_job_container(&self, job_run_id: &str, container_id: &str) -> Result<()> {
        run_queries::set_job_container(&self.db, job_run_id, container_id).await.map_err(Into::into)
    }

    async fn create_step_run(&self, job_run_id: &str, step_index: i64, name: Option<&str>, kind: &str) -> Result<String> {
        let step_run = run_queries::create_step_run(&self.db, job_run_id, step_index, name, kind).await?;
        Ok(step_run.id)
    }

    async fn set_step_status(&self, step_run_id: &str, status: &str, exit_code: Option<i64>, terminal: bool) -> Result<()> {
        run_queries::set_step_status(&self.db, step_run_id, status, exit_code, terminal).await.map_err(Into::into)
    }

    async fn insert_log_line(&self, step_run_id: &str, ts: &str, stream: &str, message: &str) -> Result<()> {
        self.log_hub
            .publish(
                &self.db,
                LogLine { step_run_id: step_run_id.to_string(), ts: ts.to_string(), stream: stream.to_string(), message: message.to_string() },
            )
            .await;
        Ok(())
    }

    async fn find_artifact_by_run_and_name(&self, workflow_run_id: &str, name: &str) -> Result<Option<String>> {
        let artifact = artifact_queries::find_by_run_and_name(&self.db, workflow_run_id, name).await?;
        Ok(artifact.map(|a| a.path_on_disk))
    }

    async fn record_artifact(
        &self,
        workflow_run_id: &str,
        job_run_id: Option<&str>,
        name: &str,
        path_on_disk: &str,
        size_bytes: i64,
    ) -> Result<()> {
        artifact_queries::create(&self.db, workflow_run_id, job_run_id, name, path_on_disk, size_bytes, None).await?;
        Ok(())
    }

    async fn list_secret_names(&self) -> Result<Vec<String>> {
        Ok(self.secrets.wrapped.keys().cloned().collect())
    }

    async fn request_secret(&self, name: &str) -> Result<Option<String>> {
        let Some((ciphertext, nonce)) = self.secrets.wrapped.get(name) else { return Ok(None) };
        let plaintext = self.secrets.key.decrypt_str(ciphertext, nonce).context("failed to decrypt secret under the ephemeral key")?;
        Ok(Some(plaintext))
    }

    async fn resource_cache_lookup(&self, cache_key: &str) -> Result<ResourceCacheState> {
        match cache_queries::find(&self.db, &self.bucket_id, cache_key).await? {
            Some(entry) => Ok(ResourceCacheState { entry_id: entry.id, status: entry.status, path_on_disk: entry.path_on_disk, is_builder: false }),
            None => Ok(ResourceCacheState { entry_id: String::new(), status: "miss".to_string(), path_on_disk: None, is_builder: false }),
        }
    }

    async fn resource_cache_begin_build(&self, cache_key: &str, shell_id: &str) -> Result<ResourceCacheState> {
        let entry = cache_queries::begin_build(&self.db, &self.bucket_id, cache_key, shell_id).await?;
        let is_builder = entry.builder_shell_id.as_deref() == Some(shell_id);
        Ok(ResourceCacheState { entry_id: entry.id, status: entry.status, path_on_disk: entry.path_on_disk, is_builder })
    }

    async fn resource_cache_heartbeat(&self, entry_id: &str) -> Result<()> {
        cache_queries::heartbeat_build(&self.db, entry_id).await.map_err(Into::into)
    }

    async fn resource_cache_complete(&self, entry_id: &str, path_on_disk: &str, size_bytes: i64) -> Result<()> {
        cache_queries::complete_build(&self.db, entry_id, path_on_disk, size_bytes).await.map_err(Into::into)
    }

    async fn resource_cache_fail(&self, entry_id: &str) -> Result<()> {
        cache_queries::fail_build(&self.db, entry_id).await.map_err(Into::into)
    }

    async fn record_job_shard(&self, id: &str, job_run_id: &str, workflow_run_id: &str, workspace_path: &str, network_enabled: bool, ttl_expires_at: &str) -> Result<()> {
        shard_queries::create(&self.db, id, job_run_id, workflow_run_id, workspace_path, network_enabled, ttl_expires_at).await?;
        Ok(())
    }

    async fn mark_shard_reaped(&self, shard_id: &str) -> Result<()> {
        shard_queries::mark_reaped(&self.db, shard_id).await.map_err(Into::into)
    }
}

/// The shell-side `RunClient`: every call is one request/response round-trip over an `atk_rcp`
/// transport to this shell's owning bucket. `stream` is behind a `Mutex` since a shell drives one
/// job DAG at a time per connection but individual steps may log concurrently.
pub struct RcpRunClient<S> {
    stream: Mutex<S>,
}

impl<S> RcpRunClient<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    /// Connects and completes the `Hello` handshake, failing fast if this bucket doesn't
    /// recognize the token (e.g. a stale spec file from a bucket that already tore down).
    pub async fn handshake(mut stream: S, bucket_id: &str, auth_token: &str) -> Result<Self> {
        atk_rcp::framing::send(&mut stream, &Hello { bucket_id: bucket_id.to_string(), auth_token: auth_token.to_string() })
            .await
            .context("failed to send RCP hello")?;
        let response: RcpResponse =
            atk_rcp::framing::recv(&mut stream).await.context("failed to read RCP hello response")?.context("bucket closed the connection during handshake")?;
        match response {
            RcpResponse::Ok => Ok(Self { stream: Mutex::new(stream) }),
            RcpResponse::Error(message) => anyhow::bail!("bucket rejected this shell's RCP handshake: {message}"),
            other => anyhow::bail!("unexpected RCP handshake response: {other:?}"),
        }
    }

    async fn call(&self, request: RcpRequest) -> Result<RcpResponse> {
        let mut stream = self.stream.lock().await;
        atk_rcp::framing::send(&mut *stream, &request).await.context("failed to send RCP request")?;
        let response = atk_rcp::framing::recv(&mut *stream).await.context("failed to read RCP response")?;
        response.context("bucket closed the RCP connection")
    }
}

#[async_trait]
impl<S> RunClient for RcpRunClient<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    async fn set_run_status(&self, workflow_run_id: &str, status: &str, terminal: bool) -> Result<()> {
        match self.call(RcpRequest::SetRunStatus { workflow_run_id: workflow_run_id.to_string(), status: status.to_string(), terminal }).await? {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to SetRunStatus: {other:?}"),
        }
    }

    async fn set_job_status(&self, job_run_id: &str, status: &str, exit_code: Option<i64>, terminal: bool) -> Result<()> {
        match self.call(RcpRequest::SetJobStatus { job_run_id: job_run_id.to_string(), status: status.to_string(), exit_code, terminal }).await? {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to SetJobStatus: {other:?}"),
        }
    }

    async fn set_job_container(&self, job_run_id: &str, container_id: &str) -> Result<()> {
        match self.call(RcpRequest::SetJobContainer { job_run_id: job_run_id.to_string(), container_id: container_id.to_string() }).await? {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to SetJobContainer: {other:?}"),
        }
    }

    async fn create_step_run(&self, job_run_id: &str, step_index: i64, name: Option<&str>, kind: &str) -> Result<String> {
        match self
            .call(RcpRequest::CreateStepRun {
                job_run_id: job_run_id.to_string(),
                step_index,
                name: name.map(str::to_string),
                kind: kind.to_string(),
            })
            .await?
        {
            RcpResponse::StepRunId(id) => Ok(id),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to CreateStepRun: {other:?}"),
        }
    }

    async fn set_step_status(&self, step_run_id: &str, status: &str, exit_code: Option<i64>, terminal: bool) -> Result<()> {
        match self
            .call(RcpRequest::SetStepStatus { step_run_id: step_run_id.to_string(), status: status.to_string(), exit_code, terminal })
            .await?
        {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to SetStepStatus: {other:?}"),
        }
    }

    async fn insert_log_line(&self, step_run_id: &str, ts: &str, stream: &str, message: &str) -> Result<()> {
        match self
            .call(RcpRequest::InsertLogLine {
                step_run_id: step_run_id.to_string(),
                ts: ts.to_string(),
                stream: stream.to_string(),
                message: message.to_string(),
            })
            .await?
        {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to InsertLogLine: {other:?}"),
        }
    }

    async fn find_artifact_by_run_and_name(&self, workflow_run_id: &str, name: &str) -> Result<Option<String>> {
        match self.call(RcpRequest::FindArtifactByRunAndName { workflow_run_id: workflow_run_id.to_string(), name: name.to_string() }).await? {
            RcpResponse::Artifact(info) => Ok(info.map(|i| i.path_on_disk)),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to FindArtifactByRunAndName: {other:?}"),
        }
    }

    async fn record_artifact(
        &self,
        workflow_run_id: &str,
        job_run_id: Option<&str>,
        name: &str,
        path_on_disk: &str,
        size_bytes: i64,
    ) -> Result<()> {
        match self
            .call(RcpRequest::RecordArtifact {
                workflow_run_id: workflow_run_id.to_string(),
                job_run_id: job_run_id.map(str::to_string),
                name: name.to_string(),
                path_on_disk: path_on_disk.to_string(),
                size_bytes,
            })
            .await?
        {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to RecordArtifact: {other:?}"),
        }
    }

    async fn list_secret_names(&self) -> Result<Vec<String>> {
        match self.call(RcpRequest::ListSecretNames).await? {
            RcpResponse::SecretNames(names) => Ok(names),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to ListSecretNames: {other:?}"),
        }
    }

    async fn request_secret(&self, name: &str) -> Result<Option<String>> {
        match self.call(RcpRequest::RequestSecret { name: name.to_string() }).await? {
            RcpResponse::Secret(value) => Ok(value),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to RequestSecret: {other:?}"),
        }
    }

    async fn resource_cache_lookup(&self, cache_key: &str) -> Result<ResourceCacheState> {
        match self.call(RcpRequest::ResourceCacheLookup { cache_key: cache_key.to_string() }).await? {
            RcpResponse::ResourceCache(state) => Ok(state),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to ResourceCacheLookup: {other:?}"),
        }
    }

    async fn resource_cache_begin_build(&self, cache_key: &str, shell_id: &str) -> Result<ResourceCacheState> {
        match self.call(RcpRequest::ResourceCacheBeginBuild { cache_key: cache_key.to_string(), shell_id: shell_id.to_string() }).await? {
            RcpResponse::ResourceCache(state) => Ok(state),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to ResourceCacheBeginBuild: {other:?}"),
        }
    }

    async fn resource_cache_heartbeat(&self, entry_id: &str) -> Result<()> {
        match self.call(RcpRequest::ResourceCacheHeartbeat { entry_id: entry_id.to_string() }).await? {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to ResourceCacheHeartbeat: {other:?}"),
        }
    }

    async fn resource_cache_complete(&self, entry_id: &str, path_on_disk: &str, size_bytes: i64) -> Result<()> {
        match self
            .call(RcpRequest::ResourceCacheComplete { entry_id: entry_id.to_string(), path_on_disk: path_on_disk.to_string(), size_bytes })
            .await?
        {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to ResourceCacheComplete: {other:?}"),
        }
    }

    async fn resource_cache_fail(&self, entry_id: &str) -> Result<()> {
        match self.call(RcpRequest::ResourceCacheFail { entry_id: entry_id.to_string() }).await? {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to ResourceCacheFail: {other:?}"),
        }
    }

    async fn record_job_shard(&self, id: &str, job_run_id: &str, workflow_run_id: &str, workspace_path: &str, network_enabled: bool, ttl_expires_at: &str) -> Result<()> {
        match self
            .call(RcpRequest::RecordJobShard {
                id: id.to_string(),
                job_run_id: job_run_id.to_string(),
                workflow_run_id: workflow_run_id.to_string(),
                workspace_path: workspace_path.to_string(),
                network_enabled,
                ttl_expires_at: ttl_expires_at.to_string(),
            })
            .await?
        {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to RecordJobShard: {other:?}"),
        }
    }

    async fn mark_shard_reaped(&self, shard_id: &str) -> Result<()> {
        match self.call(RcpRequest::MarkShardReaped { shard_id: shard_id.to_string() }).await? {
            RcpResponse::Ok => Ok(()),
            RcpResponse::Error(message) => anyhow::bail!(message),
            other => anyhow::bail!("unexpected response to MarkShardReaped: {other:?}"),
        }
    }
}

/// The one place a shell reports it's completely done: awaited after every job in its DAG has
/// reached a terminal, already-persisted status (see the `RunClient` calls above), so the bucket
/// only ever sees `outcome_persisted_at` set after the outcome genuinely is.
pub async fn report_shell_exit<S>(client: &RcpRunClient<S>, shell_id: &str, exit_code: i64) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send,
{
    match client.call(RcpRequest::ReportShellExit { shell_id: shell_id.to_string(), exit_code }).await? {
        RcpResponse::Ok => Ok(()),
        RcpResponse::Error(message) => anyhow::bail!(message),
        other => anyhow::bail!("unexpected response to ReportShellExit: {other:?}"),
    }
}
