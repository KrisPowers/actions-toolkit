//! Plain-TCP RCP transport for a bucket ↔ remote-agent-hosted shell, alongside the local
//! (named-pipe/Unix-socket) transport `local.rs` provides for same-machine shells.
//!
//! SECURITY NOTE: this is plain TCP, not yet wrapped in mTLS. The plan for this subsystem calls
//! for a control-plane-issued CA and client certificates per agent (see the agents/
//! agent_join_tokens tables and the `Hello`/`auth_token_hash` check already enforced at the RCP
//! layer), but that transport-level hardening hasn't landed yet — today the same bearer-token
//! handshake `local.rs` uses is the only thing authenticating a remote connection, over an
//! unencrypted socket. Treat cross-machine RCP traffic as trusted-network-only until the mTLS
//! layer lands; don't expose a bucket's TCP RCP port to an untrusted network.

use anyhow::{Context, Result};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};

pub struct TcpListener {
    inner: TokioTcpListener,
}

impl TcpListener {
    /// Binds `0.0.0.0:0` (an OS-assigned ephemeral port) when `bind_addr` is `None`, so a
    /// caller that doesn't care which port it got can read it back via `local_addr`.
    pub async fn bind(bind_addr: Option<&str>) -> Result<Self> {
        let addr = bind_addr.unwrap_or("0.0.0.0:0");
        let inner = TokioTcpListener::bind(addr).await.with_context(|| format!("failed to bind TCP RCP listener on {addr}"))?;
        Ok(Self { inner })
    }

    pub fn local_addr(&self) -> Result<std::net::SocketAddr> {
        self.inner.local_addr().context("failed to read the bound TCP RCP listener's local address")
    }

    pub async fn accept(&mut self) -> Result<TcpStream> {
        let (stream, _addr) = self.inner.accept().await.context("failed accepting a TCP RCP connection")?;
        stream.set_nodelay(true).ok(); // RCP is request/response, not bulk transfer; latency over throughput
        Ok(stream)
    }
}

pub async fn connect(addr: &str) -> Result<TcpStream> {
    let stream = TcpStream::connect(addr).await.with_context(|| format!("failed to connect to TCP RCP endpoint {addr}"))?;
    stream.set_nodelay(true).ok();
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing;

    #[tokio::test]
    async fn a_client_and_server_exchange_a_framed_message_over_tcp() {
        let mut listener = TcpListener::bind(Some("127.0.0.1:0")).await.expect("bind should succeed");
        let addr = listener.local_addr().expect("local_addr should succeed");

        let server_task = tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept should succeed");
            let received: String = framing::recv(&mut stream).await.expect("recv should succeed").expect("expected a message");
            framing::send(&mut stream, &format!("echo: {received}")).await.expect("send should succeed");
        });

        let mut client = connect(&addr.to_string()).await.expect("connect should succeed");
        framing::send(&mut client, &"hello".to_string()).await.expect("send should succeed");
        let reply: String = framing::recv(&mut client).await.expect("recv should succeed").expect("expected a reply");

        assert_eq!(reply, "echo: hello");
        server_task.await.expect("server task should not panic");
    }
}
