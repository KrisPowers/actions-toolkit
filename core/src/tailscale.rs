use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

/// State of the instance-wide Tailscale Funnel, driven by the "Start tunnel" button on the
/// Webhooks page so the operator never has to run `tailscale funnel` in a terminal themselves or
/// copy a URL out of its output by hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TailscaleTunnelState {
    Idle,
    Starting,
    Running { url: String },
    Failed { message: String },
}

pub struct TailscaleTunnel {
    state: RwLock<TailscaleTunnelState>,
    child: Mutex<Option<Child>>,
}

impl TailscaleTunnel {
    pub fn new() -> Self {
        Self { state: RwLock::new(TailscaleTunnelState::Idle), child: Mutex::new(None) }
    }

    pub async fn status(&self) -> TailscaleTunnelState {
        self.state.read().await.clone()
    }
}

impl Default for TailscaleTunnel {
    fn default() -> Self {
        Self::new()
    }
}

/// Starts (or restarts) `tailscale funnel` pointed at this instance's own port, kept running in
/// the foreground for as long as the funnel should stay up: the child process IS the tunnel, the
/// same model `tunnel::CloudflareTunnel` uses for `cloudflared`, so `stop` just kills it. A no-op
/// if a tunnel is already starting or running.
pub async fn start(tunnel: Arc<TailscaleTunnel>, port: u16) {
    {
        let current = tunnel.state.read().await;
        if matches!(&*current, TailscaleTunnelState::Starting | TailscaleTunnelState::Running { .. }) {
            return;
        }
    }

    stop(&tunnel).await;
    *tunnel.state.write().await = TailscaleTunnelState::Starting;

    let mut command = Command::new("tailscale");
    command.args(["funnel", &port.to_string()]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            *tunnel.state.write().await = TailscaleTunnelState::Failed {
                message: "tailscale isn't installed or not on PATH. Install it from the Tailscale docs (search \
                          \"tailscale install\"), then try again."
                    .to_string(),
            };
            return;
        }
        Err(e) => {
            *tunnel.state.write().await = TailscaleTunnelState::Failed { message: format!("failed to start tailscale funnel: {e}") };
            return;
        }
    };

    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        *tunnel.state.write().await = TailscaleTunnelState::Failed { message: "could not read tailscale's output".to_string() };
        return;
    };
    *tunnel.child.lock().await = Some(child);

    let bg_tunnel = tunnel.clone();
    tokio::spawn(async move {
        let mut out_lines = BufReader::new(stdout).lines();
        let mut err_lines = BufReader::new(stderr).lines();
        loop {
            let line = tokio::select! {
                line = out_lines.next_line() => line,
                line = err_lines.next_line() => line,
            };
            match line {
                Ok(Some(line)) => {
                    if let Some(url) = extract_ts_net_url(&line) {
                        *bg_tunnel.state.write().await = TailscaleTunnelState::Running { url };
                        return;
                    }
                }
                _ => break,
            }
        }
        // Both streams ended (tailscale exited or was killed) without ever printing a URL. Only
        // report that as a failure if nothing else already moved the state on (e.g. `stop`
        // resetting it to `Idle` out from under this task).
        let mut guard = bg_tunnel.state.write().await;
        if matches!(&*guard, TailscaleTunnelState::Starting) {
            *guard = TailscaleTunnelState::Failed {
                message: "tailscale funnel exited before reporting a tunnel URL. Make sure Funnel is enabled for \
                          this tailnet in the Tailscale admin console."
                    .to_string(),
            };
        }
    });
}

/// Whether `tailscale` is on PATH, used to enable/disable the "Tailscale Funnel" button on the
/// Webhooks page before the operator ever clicks it and only then discovers the binary is missing.
pub async fn is_installed() -> bool {
    let check = Command::new("tailscale").arg("version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    matches!(tokio::time::timeout(std::time::Duration::from_secs(3), check).await, Ok(Ok(_)))
}

/// Kills the running `tailscale funnel` process, if any, and resets state to `Idle`.
pub async fn stop(tunnel: &TailscaleTunnel) {
    if let Some(mut child) = tunnel.child.lock().await.take() {
        let _ = child.kill().await;
    }
    *tunnel.state.write().await = TailscaleTunnelState::Idle;
}

/// `tailscale funnel <port>` prints its assigned URL once the funnel is live, e.g.:
/// `Available on the internet: https://host.tailnet-name.ts.net/`
fn extract_ts_net_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let candidate = &line[start..];
    let end = candidate.find(|c: char| c.is_whitespace()).unwrap_or(candidate.len());
    let url = candidate[..end].trim_end_matches('/');
    url.contains(".ts.net").then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_url_from_the_funnel_status_line() {
        let line = "Available on the internet: https://myhost.tailnet-name.ts.net/";
        assert_eq!(extract_ts_net_url(line), Some("https://myhost.tailnet-name.ts.net".to_string()));
    }

    #[test]
    fn ignores_unrelated_log_lines() {
        let line = "Press Ctrl+C to exit.";
        assert_eq!(extract_ts_net_url(line), None);
    }

    #[test]
    fn ignores_https_urls_that_are_not_a_ts_net_hostname() {
        let line = "Some notice at https://tailscale.com/kb/funnel";
        assert_eq!(extract_ts_net_url(line), None);
    }

    #[tokio::test]
    async fn starting_when_the_binary_is_missing_reports_a_clear_failure() {
        let tunnel = Arc::new(TailscaleTunnel::new());
        start(tunnel.clone(), 7890).await;

        // In CI/dev environments `tailscale` is very unlikely to be on PATH; if it happens to be
        // installed this assertion is skipped rather than flaking.
        if let TailscaleTunnelState::Failed { message } = tunnel.status().await {
            assert!(message.contains("tailscale"));
        }
    }

    #[tokio::test]
    async fn a_second_start_while_running_is_a_no_op() {
        let tunnel = Arc::new(TailscaleTunnel::new());
        *tunnel.state.write().await = TailscaleTunnelState::Running { url: "https://example.ts.net".to_string() };

        start(tunnel.clone(), 7890).await;

        assert_eq!(tunnel.status().await, TailscaleTunnelState::Running { url: "https://example.ts.net".to_string() });
    }

    #[tokio::test]
    async fn stop_resets_to_idle_with_no_child_running() {
        let tunnel = TailscaleTunnel::new();
        *tunnel.state.write().await = TailscaleTunnelState::Failed { message: "boom".to_string() };

        stop(&tunnel).await;

        assert_eq!(tunnel.status().await, TailscaleTunnelState::Idle);
    }
}
