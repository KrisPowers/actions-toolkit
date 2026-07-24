use std::net::SocketAddr;

/// Best-effort client IP for login-event logging and rate limiting. Prefers `X-Forwarded-For`
/// (set by a tunnel/reverse proxy terminating the real connection in front of this instance,
/// same reasoning as `api::request_origin`'s use of `X-Forwarded-Proto`) over the raw TCP
/// peer address, which behind a tunnel would just be the tunnel's local endpoint.
pub fn client_ip(headers: &axum::http::HeaderMap, peer: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| peer.ip().to_string())
}
