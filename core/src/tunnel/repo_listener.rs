use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::app::AppState;

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

/// Binds a fresh `127.0.0.1` loopback listener (not `0.0.0.0`: the only intended caller is the
/// tunnel binary running on this same host) serving `router`, and spawns it in the background
/// with graceful shutdown wired to the returned handle.
pub async fn spawn_with_router(router: Router) -> anyhow::Result<ListenerHandle> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let local_port = listener.local_addr()?.port();

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router.into_make_service())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    Ok(ListenerHandle { local_port, shutdown_tx: Some(shutdown_tx) })
}

/// Builds and binds a loopback listener serving ONLY `repo_id`'s webhook route -- a literal path
/// baked into the router at construction time, not the generic `{repo_id}` template the main
/// listener uses, so this listener is structurally incapable of routing a request to any other
/// repo no matter what path a caller sends it.
pub async fn spawn(state: AppState, repo_id: String) -> anyhow::Result<ListenerHandle> {
    let path = format!("/webhooks/github/{repo_id}");
    let router = Router::new()
        .route(
            &path,
            post({
                let repo_id = repo_id.clone();
                move |State(state): State<AppState>, headers: HeaderMap, body: axum::body::Bytes| {
                    let repo_id = repo_id.clone();
                    async move { crate::api::webhooks::receive_for_repo(state, repo_id, headers, body).await }
                }
            }),
        )
        .with_state(state);

    spawn_with_router(router).await
}
