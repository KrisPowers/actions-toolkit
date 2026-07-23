//! Length-prefixed JSON framing over any async byte stream. Deliberately not HTTP/WS: a Windows
//! named pipe's connection semantics (single client per pipe instance, explicit reconnect loop)
//! don't map cleanly onto axum/hyper, so RCP uses this small custom protocol on both the Unix
//! socket and named-pipe transports instead of forcing HTTP semantics onto one of them.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Frames larger than this are rejected rather than read, so a corrupted or malicious length
/// prefix can't make a peer allocate an unbounded buffer.
const MAX_FRAME_BYTES: u32 = 64 * 1024 * 1024;

pub async fn send<W: AsyncWrite + Unpin, T: Serialize>(writer: &mut W, message: &T) -> Result<()> {
    let payload = serde_json::to_vec(message).context("failed to serialize RCP message")?;
    let len = u32::try_from(payload.len()).context("RCP message too large to frame")?;
    writer.write_all(&len.to_be_bytes()).await.context("failed to write RCP frame length")?;
    writer.write_all(&payload).await.context("failed to write RCP frame body")?;
    writer.flush().await.context("failed to flush RCP frame")?;
    Ok(())
}

/// Returns `Ok(None)` on a clean EOF before any bytes of the next frame arrive (the peer closed
/// the connection between messages, not mid-frame), which callers treat as "the other side hung
/// up" rather than an error.
pub async fn recv<R: AsyncRead + Unpin, T: DeserializeOwned>(reader: &mut R) -> Result<Option<T>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e).context("failed to read RCP frame length"),
    }
    let len = u32::from_be_bytes(len_buf);
    anyhow::ensure!(len <= MAX_FRAME_BYTES, "RCP frame of {len} bytes exceeds the {MAX_FRAME_BYTES} byte limit");

    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload).await.context("failed to read RCP frame body")?;
    let message = serde_json::from_slice(&payload).context("failed to deserialize RCP message")?;
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Ping {
        n: u32,
    }

    #[tokio::test]
    async fn a_sent_message_round_trips_through_recv() {
        let mut buf = Vec::new();
        send(&mut buf, &Ping { n: 42 }).await.unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let received: Ping = recv(&mut cursor).await.unwrap().expect("expected a message, not EOF");
        assert_eq!(received, Ping { n: 42 });
    }

    #[tokio::test]
    async fn recv_reports_none_on_a_clean_eof_between_frames() {
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let received: Option<Ping> = recv(&mut cursor).await.unwrap();
        assert_eq!(received, None);
    }

    #[tokio::test]
    async fn recv_rejects_a_frame_length_over_the_limit() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(MAX_FRAME_BYTES + 1).to_be_bytes());
        let mut cursor = std::io::Cursor::new(buf);
        let result: Result<Option<Ping>> = recv(&mut cursor).await;
        assert!(result.is_err(), "expected an oversized frame length to be rejected before reading the body");
    }
}
