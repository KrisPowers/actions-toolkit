use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{TunnelProcess, TunnelState};

/// Starts (or restarts) `tailscale funnel` pointed at `port`, kept running in the foreground for
/// as long as the funnel should stay up: the child process IS the tunnel, same model
/// `cloudflare::start` uses. A no-op if `process` is already starting or running.
pub async fn start(process: Arc<TunnelProcess>, port: u16) {
    {
        let current = process.state.read().await;
        if matches!(&*current, TunnelState::Starting | TunnelState::Running { .. }) {
            return;
        }
    }

    super::stop(&process).await;
    *process.state.write().await = TunnelState::Starting;

    let mut command = Command::new("tailscale");
    command.args(["funnel", &port.to_string()]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            *process.state.write().await = TunnelState::Failed {
                message: "tailscale isn't installed or not on PATH. Install it from the Tailscale docs (search \
                          \"tailscale install\"), then try again."
                    .to_string(),
            };
            return;
        }
        Err(e) => {
            *process.state.write().await = TunnelState::Failed { message: format!("failed to start tailscale funnel: {e}") };
            return;
        }
    };

    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        *process.state.write().await = TunnelState::Failed { message: "could not read tailscale's output".to_string() };
        return;
    };
    *process.child.lock().await = Some(child);

    let bg_process = process.clone();
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
                        *bg_process.state.write().await = TunnelState::Running { url };
                        return;
                    }
                }
                _ => break,
            }
        }
        // Both streams ended (tailscale exited or was killed) without ever printing a URL. Only
        // report that as a failure if nothing else already moved the state on (e.g. `stop`
        // resetting it to `Idle` out from under this task).
        let mut guard = bg_process.state.write().await;
        if matches!(&*guard, TunnelState::Starting) {
            *guard = TunnelState::Failed {
                message: "tailscale funnel exited before reporting a tunnel URL. Make sure Funnel is enabled for \
                          this tailnet in the Tailscale admin console."
                    .to_string(),
            };
        }
    });
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
