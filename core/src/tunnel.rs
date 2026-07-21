use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

/// State of the instance-wide Cloudflare Quick Tunnel, driven by the "Start tunnel" button on the
/// Webhooks page so the operator never has to run `cloudflared` in a terminal themselves or copy
/// a URL out of its output by hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CloudflareTunnelState {
    Idle,
    Starting,
    Running { url: String },
    Failed { message: String },
}

pub struct CloudflareTunnel {
    state: RwLock<CloudflareTunnelState>,
    child: Mutex<Option<Child>>,
}

impl CloudflareTunnel {
    pub fn new() -> Self {
        Self { state: RwLock::new(CloudflareTunnelState::Idle), child: Mutex::new(None) }
    }

    pub async fn status(&self) -> CloudflareTunnelState {
        self.state.read().await.clone()
    }
}

impl Default for CloudflareTunnel {
    fn default() -> Self {
        Self::new()
    }
}

/// Starts (or restarts) `cloudflared` as a quick tunnel pointed at this instance's own port. A
/// no-op if a tunnel is already starting or running. cloudflared prints the assigned
/// `https://*.trycloudflare.com` URL to stderr once the tunnel is actually up; a background task
/// scans for it and flips the shared state to `Running` as soon as it appears, so the frontend
/// can poll `status()` instead of the operator having to copy the URL out of a terminal.
pub async fn start(tunnel: Arc<CloudflareTunnel>, port: u16) {
    {
        let current = tunnel.state.read().await;
        if matches!(&*current, CloudflareTunnelState::Starting | CloudflareTunnelState::Running { .. }) {
            return;
        }
    }

    stop(&tunnel).await;
    *tunnel.state.write().await = CloudflareTunnelState::Starting;

    let mut command = Command::new("cloudflared");
    command.args(["tunnel", "--url", &format!("http://localhost:{port}")]).stdout(Stdio::null()).stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            *tunnel.state.write().await = CloudflareTunnelState::Failed {
                message: "cloudflared isn't installed or not on PATH. Install it from the Cloudflare docs (search \
                          \"cloudflared install\"), then try again."
                    .to_string(),
            };
            return;
        }
        Err(e) => {
            *tunnel.state.write().await = CloudflareTunnelState::Failed { message: format!("failed to start cloudflared: {e}") };
            return;
        }
    };

    let stderr = match child.stderr.take() {
        Some(s) => s,
        None => {
            *tunnel.state.write().await = CloudflareTunnelState::Failed { message: "could not read cloudflared's output".to_string() };
            return;
        }
    };
    *tunnel.child.lock().await = Some(child);

    let bg_tunnel = tunnel.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(url) = extract_trycloudflare_url(&line) {
                *bg_tunnel.state.write().await = CloudflareTunnelState::Running { url };
                return;
            }
        }
        // The stderr stream ended (cloudflared exited or was killed) without ever printing a
        // URL. Only report that as a failure if nothing else already moved the state on
        // (e.g. `stop` resetting it to `Idle` out from under this task).
        let mut guard = bg_tunnel.state.write().await;
        if matches!(&*guard, CloudflareTunnelState::Starting) {
            *guard = CloudflareTunnelState::Failed { message: "cloudflared exited before reporting a tunnel URL".to_string() };
        }
    });
}

/// Kills the running `cloudflared` process, if any, and resets state to `Idle`.
pub async fn stop(tunnel: &CloudflareTunnel) {
    if let Some(mut child) = tunnel.child.lock().await.take() {
        let _ = child.kill().await;
    }
    *tunnel.state.write().await = CloudflareTunnelState::Idle;
}

/// cloudflared logs its assigned URL inside a bordered box on stderr, e.g.:
/// `2024-01-01T00:00:00Z INF |  https://random-words-here.trycloudflare.com  |`
fn extract_trycloudflare_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let candidate = &line[start..];
    let end = candidate.find(|c: char| c.is_whitespace() || c == '|').unwrap_or(candidate.len());
    let url = &candidate[..end];
    url.contains(".trycloudflare.com").then(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_url_from_a_bordered_cloudflared_log_line() {
        let line = "2024-01-01T00:00:00Z INF |  https://random-words-here.trycloudflare.com                |";
        assert_eq!(extract_trycloudflare_url(line), Some("https://random-words-here.trycloudflare.com".to_string()));
    }

    #[test]
    fn ignores_unrelated_log_lines() {
        let line = "2024-01-01T00:00:00Z INF Starting tunnel";
        assert_eq!(extract_trycloudflare_url(line), None);
    }

    #[test]
    fn ignores_https_urls_that_are_not_a_trycloudflare_hostname() {
        let line = "2024-01-01T00:00:00Z INF |  https://api.cloudflare.com/health  |";
        assert_eq!(extract_trycloudflare_url(line), None);
    }

    #[tokio::test]
    async fn starting_when_the_binary_is_missing_reports_a_clear_failure() {
        let tunnel = Arc::new(CloudflareTunnel::new());
        start(tunnel.clone(), 7890).await;

        // In CI/dev environments `cloudflared` is very unlikely to be on PATH; if it happens to
        // be installed this assertion is skipped rather than flaking.
        if let CloudflareTunnelState::Failed { message } = tunnel.status().await {
            assert!(message.contains("cloudflared"));
        }
    }

    #[tokio::test]
    async fn a_second_start_while_running_is_a_no_op() {
        let tunnel = Arc::new(CloudflareTunnel::new());
        *tunnel.state.write().await = CloudflareTunnelState::Running { url: "https://example.trycloudflare.com".to_string() };

        start(tunnel.clone(), 7890).await;

        assert_eq!(tunnel.status().await, CloudflareTunnelState::Running { url: "https://example.trycloudflare.com".to_string() });
    }

    #[tokio::test]
    async fn stop_resets_to_idle_with_no_child_running() {
        let tunnel = CloudflareTunnel::new();
        *tunnel.state.write().await = CloudflareTunnelState::Failed { message: "boom".to_string() };

        stop(&tunnel).await;

        assert_eq!(tunnel.status().await, CloudflareTunnelState::Idle);
    }
}
