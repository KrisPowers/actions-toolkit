use tokio::sync::oneshot;

/// A running loopback listener -- either one repo's dedicated webhook listener or the dashboard
/// tunnel's own listener. Whichever it is, its lifecycle is independent of every other listener.
pub struct ListenerHandle {
    pub local_port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl ListenerHandle {
    /// Shuts the listener down, freeing its port. Idempotent -- a second call is a no-op since
    /// the shutdown signal can only be sent once.
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
