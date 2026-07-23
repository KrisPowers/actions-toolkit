//! The worker-agent runtime (`actions-toolkit agent`): registers this machine with a control
//! plane, then polls for shells scheduled onto it and runs them locally. Intentionally reuses the
//! exact same `__shell-run` subcommand a locally-spawned shell uses — a shell binary doesn't know
//! or care whether its parent is the control plane or an agent, it just connects to whatever RCP
//! endpoint its spec names, which for an agent-hosted shell is the bucket's TCP listener rather
//! than a local named pipe.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use atk_config::AgentArgs;

use crate::runner::shell_run::ShellRunSpec;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize, Deserialize)]
struct AgentIdentity {
    agent_id: String,
    auth_token: String,
}

pub async fn run(args: AgentArgs) -> Result<()> {
    let data_dir = atk_config::resolve_data_dir(args.data_dir.clone());
    std::fs::create_dir_all(&data_dir).context("failed to create agent data directory")?;
    let workspaces_dir = data_dir.join("workspaces");
    let buckets_dir = data_dir.join("buckets");
    let artifacts_dir = data_dir.join("artifacts");
    std::fs::create_dir_all(&workspaces_dir)?;
    std::fs::create_dir_all(&buckets_dir)?;
    std::fs::create_dir_all(&artifacts_dir)?;

    let capability = atk_bucket::probe_capability().await;
    if !capability.ok {
        anyhow::bail!(
            "this host cannot run job sandboxes ({}); an agent has nowhere to run jobs without one",
            capability.reason.as_deref().unwrap_or("unknown reason")
        );
    }

    let client = reqwest::Client::new();
    let identity_path = data_dir.join("agent-identity.json");
    let identity = load_or_join(&client, &args, &identity_path).await?;

    tracing::info!(agent_id = %identity.agent_id, control_plane = %args.control_plane_url, "agent registered, starting poll loop");

    let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    loop {
        interval.tick().await;

        if let Err(e) = heartbeat(&client, &args.control_plane_url, &identity).await {
            tracing::warn!(error = %e, "heartbeat failed, will retry next interval");
        }

        match poll_assignments(&client, &args.control_plane_url, &identity).await {
            Ok(assignments) => {
                for assignment in assignments {
                    if let Err(e) = run_assignment(&client, &args.control_plane_url, &identity, &assignment, &workspaces_dir, &buckets_dir, &artifacts_dir).await
                    {
                        tracing::error!(error = %e, shell_id = %assignment.shell_id, "failed to start an assigned shell");
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to poll for assigned shells"),
        }
    }
}

async fn load_or_join(client: &reqwest::Client, args: &AgentArgs, identity_path: &PathBuf) -> Result<AgentIdentity> {
    if identity_path.exists() {
        let bytes = std::fs::read(identity_path).context("failed to read persisted agent identity")?;
        return serde_json::from_slice(&bytes).context("failed to parse persisted agent identity");
    }

    let join_token = args.join_token.as_deref().context("no persisted agent identity found and no --join-token was given")?;
    let name = args.name.clone().unwrap_or_else(hostname);
    let labels_with_defaults: Vec<String> =
        [format!("os={}", std::env::consts::OS), format!("arch={}", std::env::consts::ARCH)].into_iter().chain(args.labels.clone()).collect();

    let response = client
        .post(format!("{}/api/agents/join", args.control_plane_url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "token": join_token,
            "name": name,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "labels": labels_with_defaults,
        }))
        .send()
        .await
        .context("failed to reach the control plane's join endpoint")?
        .error_for_status()
        .context("the control plane rejected this join attempt (a wrong, expired, or already-used token gives the same error)")?;

    #[derive(Deserialize)]
    struct JoinResponse {
        agent_id: String,
        auth_token: String,
    }
    let joined: JoinResponse = response.json().await.context("failed to parse the control plane's join response")?;
    let identity = AgentIdentity { agent_id: joined.agent_id, auth_token: joined.auth_token };

    std::fs::write(identity_path, serde_json::to_vec(&identity)?).context("failed to persist this agent's issued identity")?;
    tracing::warn!(
        path = %identity_path.display(),
        "issued a new agent identity; this machine won't need --join-token again as long as this file exists, but an operator still has to approve it in the Agents UI before it's used for real jobs"
    );

    Ok(identity)
}

async fn heartbeat(client: &reqwest::Client, control_plane_url: &str, identity: &AgentIdentity) -> Result<()> {
    client
        .post(format!("{}/api/agents/{}/heartbeat", control_plane_url.trim_end_matches('/'), identity.agent_id))
        .bearer_auth(&identity.auth_token)
        .json(&serde_json::json!({ "capacity": 1, "version": AGENT_VERSION }))
        .send()
        .await
        .context("heartbeat request failed")?
        .error_for_status()
        .context("control plane rejected the heartbeat")?;
    Ok(())
}

#[derive(Deserialize)]
struct Assignment {
    shell_id: String,
    #[allow(dead_code)]
    workflow_run_id: String,
}

async fn poll_assignments(client: &reqwest::Client, control_plane_url: &str, identity: &AgentIdentity) -> Result<Vec<Assignment>> {
    let response = client
        .get(format!("{}/api/agents/{}/assignments", control_plane_url.trim_end_matches('/'), identity.agent_id))
        .bearer_auth(&identity.auth_token)
        .send()
        .await
        .context("assignments poll request failed")?
        .error_for_status()
        .context("control plane rejected the assignments poll")?;
    response.json().await.context("failed to parse the assignments response")
}

#[allow(clippy::too_many_arguments)]
async fn run_assignment(
    client: &reqwest::Client,
    control_plane_url: &str,
    identity: &AgentIdentity,
    assignment: &Assignment,
    workspaces_dir: &std::path::Path,
    buckets_dir: &std::path::Path,
    artifacts_dir: &std::path::Path,
) -> Result<()> {
    let spec_text = client
        .get(format!("{}/api/agents/{}/shells/{}/spec", control_plane_url.trim_end_matches('/'), identity.agent_id, assignment.shell_id))
        .bearer_auth(&identity.auth_token)
        .send()
        .await
        .context("failed to fetch this shell's spec")?
        .error_for_status()
        .context("control plane rejected the spec fetch")?
        .text()
        .await
        .context("failed to read the spec response body")?;

    let mut spec: ShellRunSpec = serde_json::from_str(&spec_text).context("failed to parse the fetched shell run spec")?;
    // The control plane wrote its own local paths into these fields; a shell running on this
    // agent's machine needs this machine's paths instead.
    spec.workspaces_dir = workspaces_dir.to_path_buf();
    spec.buckets_dir = buckets_dir.to_path_buf();
    spec.artifacts_dir = artifacts_dir.to_path_buf();

    let spec_path = buckets_dir.join(format!("shell-spec-{}.json", assignment.shell_id));
    std::fs::write(&spec_path, serde_json::to_vec(&spec)?).context("failed to write the local shell run spec")?;

    let current_exe = std::env::current_exe().context("failed to resolve current executable path")?;
    let child = tokio::process::Command::new(&current_exe)
        .arg("__shell-run")
        .arg(&spec_path)
        .spawn()
        .context("failed to spawn shell subprocess")?;

    if let Some(pid) = child.id() {
        client
            .post(format!(
                "{}/api/agents/{}/shells/{}/started",
                control_plane_url.trim_end_matches('/'),
                identity.agent_id,
                assignment.shell_id
            ))
            .bearer_auth(&identity.auth_token)
            .json(&serde_json::json!({ "pid": pid }))
            .send()
            .await
            .context("failed to report this shell as started")?
            .error_for_status()
            .context("control plane rejected the started report")?;
    }

    // Deliberately not awaited: this agent's job is to spawn the process and get out of the way,
    // same as `dispatch.rs` does for a locally-spawned shell. The shell reports its own exit to
    // its bucket directly over RCP; Janga's reconciliation sweep (see `atk_bucket::reaper`) is
    // what notices if this agent itself restarts with the child still (or no longer) running.
    drop(child);

    Ok(())
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "agent".to_string())
}
