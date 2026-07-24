use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;

use crate::app::AppState;
use crate::auth::middleware::peek_user_id;

/// Layered on the dashboard tunnel's hardened router ONLY -- never on the plain LAN listener --
/// so remote, internet-facing traffic gets meaningfully stricter treatment than someone already
/// on the local network: a blunt per-IP allowance for pre-auth requests, a finer per-(IP, user)
/// allowance once a session cookie is present, and an audit record of every request regardless of
/// outcome. Runs ahead of any handler-level extractor (tower middleware executes before axum
/// extractors), so it can't just ask for `ApprovedUser` -- it peeks the session itself via
/// `peek_user_id` instead of duplicating `CurrentUser`'s DB validation.
pub async fn guard(State(state): State<AppState>, ConnectInfo(addr): ConnectInfo<SocketAddr>, req: Request, next: Next) -> Response {
    let ip = crate::net::client_ip(req.headers(), addr);
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    if !state.dashboard_tunnel.ip_limiter.check(&ip) {
        return reject(&state, Some(ip), None, method, path, StatusCode::TOO_MANY_REQUESTS).await;
    }

    let user_id = peek_user_id(req.headers(), &state);
    if let Some(uid) = &user_id {
        let key = format!("{ip}|{uid}");
        if !state.dashboard_tunnel.user_limiter.check(&key) {
            return reject(&state, Some(ip), Some(uid.clone()), method, path, StatusCode::TOO_MANY_REQUESTS).await;
        }
    }

    let response = next.run(req).await;
    state.dashboard_tunnel.record_request(&state.db, Some(ip), user_id, method, path, response.status().as_u16(), false).await;
    response
}
