use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

const NEEDS_RECONNECT_REASON: &str = "needs_reconnect";

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    /// The stored GitHub connection can't authenticate a call right now (a `pat` row past the
    /// legacy migration cutoff, or a `github_app` row whose refresh attempt failed) and needs
    /// the user to reconnect through Settings. Distinct from a bare `Unauthorized` so the
    /// frontend can key off `reason: "needs_reconnect"` and show a specific prompt instead of a
    /// generic auth error.
    #[error("GitHub connection needs to be reconnected: {0}")]
    NeedsReconnect(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::NeedsReconnect(_) = &self {
            return (StatusCode::UNAUTHORIZED, Json(json!({ "error": self.to_string(), "reason": NEEDS_RECONNECT_REASON }))).into_response();
        }

        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            AppError::NeedsReconnect(_) => unreachable!("handled above"),
            AppError::Database(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
            AppError::Internal(e) => {
                tracing::error!(error = %e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
