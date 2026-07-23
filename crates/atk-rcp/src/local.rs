//! Local-machine RCP transport: a Unix domain socket on Linux/macOS, a named pipe on Windows.
//! Used for control-plane-to-local-shell traffic today, and (once an agent runs co-located with
//! its shells) for shell-to-local-agent traffic that the agent then relays to the control plane
//! over `MtlsTransport`.

use anyhow::{Context, Result};

/// A unique, per-bucket local endpoint name/path. Callers derive this from the bucket id so two
/// buckets never collide on the same pipe name / socket path.
pub fn endpoint_for_bucket(bucket_id: &str) -> String {
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\atk-bucket-{bucket_id}")
    }
    #[cfg(unix)]
    {
        std::env::temp_dir().join(format!("atk-bucket-{bucket_id}.sock")).to_string_lossy().into_owned()
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, PipeMode, ServerOptions};

    pub struct LocalListener {
        name: String,
        next: NamedPipeServer,
    }

    impl LocalListener {
        pub fn bind(endpoint: &str) -> Result<Self> {
            let next = ServerOptions::new()
                .pipe_mode(PipeMode::Byte)
                .first_pipe_instance(true)
                .create(endpoint)
                .with_context(|| format!("failed to create named pipe {endpoint}"))?;
            Ok(Self { name: endpoint.to_string(), next })
        }

        pub async fn accept(&mut self) -> Result<NamedPipeServer> {
            self.next.connect().await.context("failed waiting for a named pipe client to connect")?;
            let connected = std::mem::replace(
                &mut self.next,
                ServerOptions::new()
                    .pipe_mode(PipeMode::Byte)
                    .create(&self.name)
                    .with_context(|| format!("failed to create the next named pipe instance for {}", self.name))?,
            );
            Ok(connected)
        }
    }

    pub async fn connect(endpoint: &str) -> Result<tokio::net::windows::named_pipe::NamedPipeClient> {
        // A pipe server that's mid-way through handling a previous client (or hasn't rotated to
        // its next instance yet) makes a fresh connect attempt fail with ERROR_PIPE_BUSY, not a
        // permanent failure; a short retry loop is the standard client-side pattern for named
        // pipes, matching what `WaitNamedPipe` is for in the raw Win32 API.
        let mut last_err = None;
        for attempt in 0..50 {
            match ClientOptions::new().open(endpoint) {
                Ok(client) => return Ok(client),
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(std::time::Duration::from_millis(20 * (attempt + 1).min(10))).await;
                }
            }
        }
        Err(last_err.unwrap()).with_context(|| format!("failed to connect to named pipe {endpoint} after retrying"))
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use tokio::net::{UnixListener, UnixStream};

    pub struct LocalListener {
        inner: UnixListener,
        path: std::path::PathBuf,
    }

    impl LocalListener {
        pub fn bind(endpoint: &str) -> Result<Self> {
            let path = std::path::PathBuf::from(endpoint);
            let _ = std::fs::remove_file(&path); // a stale socket file from a crashed prior process
            let inner = UnixListener::bind(&path).with_context(|| format!("failed to bind unix socket {endpoint}"))?;
            Ok(Self { inner, path })
        }

        pub async fn accept(&mut self) -> Result<UnixStream> {
            let (stream, _addr) = self.inner.accept().await.context("failed accepting a unix socket connection")?;
            Ok(stream)
        }
    }

    impl Drop for LocalListener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub async fn connect(endpoint: &str) -> Result<UnixStream> {
        UnixStream::connect(endpoint).await.with_context(|| format!("failed to connect to unix socket {endpoint}"))
    }
}

pub use imp::{connect, LocalListener};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing;

    #[tokio::test]
    async fn a_client_and_server_exchange_a_framed_message_over_the_local_transport() {
        let endpoint = endpoint_for_bucket(&uuid::Uuid::new_v4().to_string());
        let mut listener = LocalListener::bind(&endpoint).expect("bind should succeed");

        let endpoint_for_client = endpoint.clone();
        let server_task = tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept should succeed");
            let received: String = framing::recv(&mut stream).await.expect("recv should succeed").expect("expected a message");
            framing::send(&mut stream, &format!("echo: {received}")).await.expect("send should succeed");
        });

        // The listener needs a moment to be actively waiting on `connect()`/`accept()` before a
        // client dials in; the named-pipe client retry loop already covers this on Windows, and a
        // fresh unix socket accepts immediately once bound, so this is a small, generous margin
        // rather than a tight race.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = connect(&endpoint_for_client).await.expect("connect should succeed");
        framing::send(&mut client, &"hello".to_string()).await.expect("send should succeed");
        let reply: String = framing::recv(&mut client).await.expect("recv should succeed").expect("expected a reply");

        assert_eq!(reply, "echo: hello");
        server_task.await.expect("server task should not panic");
    }
}
