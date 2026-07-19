use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::app::AppState;
use crate::auth::middleware::CurrentUser;
use crate::config::GITHUB_APP_SLUG;
use crate::db::queries::github_token as token_queries;
use crate::error::{AppError, AppResult};
use crate::github::{client, discovery, oauth};

#[derive(Serialize)]
pub struct DeviceStartResponse {
    pub user_code: String,
    pub verification_uri: String,
    pub interval: i64,
    pub expires_in: i64,
}

/// Starts a device-flow connect attempt. Gated behind `CurrentUser` so only a logged-in operator
/// can kick off a connect that would replace the instance-wide GitHub connection.
pub async fn device_start(State(state): State<AppState>, CurrentUser(_user): CurrentUser) -> AppResult<Json<DeviceStartResponse>> {
    let started =
        oauth::start_device_flow(&state.config.github_device_code_url, &state.config.github_app_client_id).await.map_err(AppError::Internal)?;

    *state.pending_device_flow.write().await = Some(oauth::PendingDeviceFlow {
        device_code: started.device_code,
        interval_secs: started.interval,
        expires_at: chrono::Utc::now() + chrono::Duration::seconds(started.expires_in),
    });

    Ok(Json(DeviceStartResponse {
        user_code: started.user_code,
        verification_uri: started.verification_uri,
        interval: started.interval,
        expires_in: started.expires_in,
    }))
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DevicePollResponse {
    Pending,
    Denied,
    Expired,
    /// No connect attempt is in progress (nothing to poll, or it already finished/expired and
    /// was cleared), distinct from `Expired` so the frontend doesn't show a stale "expired"
    /// message for a poll that arrives after the flow already completed successfully.
    NotStarted,
    Connected {
        github_login: String,
        has_installation: bool,
    },
}

/// Polls once for whether the in-flight device-flow attempt has been approved. The frontend
/// calls this on a timer at the interval `device_start` returned. Any terminal outcome (denied,
/// expired, connected, or an unexpected error) clears the pending attempt so a stale poll can't
/// resurrect it.
pub async fn device_poll(State(state): State<AppState>, CurrentUser(_user): CurrentUser) -> AppResult<Json<DevicePollResponse>> {
    let Some(pending) = state.pending_device_flow.read().await.clone() else {
        return Ok(Json(DevicePollResponse::NotStarted));
    };

    if chrono::Utc::now() > pending.expires_at {
        *state.pending_device_flow.write().await = None;
        return Ok(Json(DevicePollResponse::Expired));
    }

    match oauth::poll_device_token(&state.config.github_oauth_token_url, &state.config.github_app_client_id, &pending.device_code).await {
        Ok(oauth::DevicePollOutcome::Pending) => Ok(Json(DevicePollResponse::Pending)),
        Ok(oauth::DevicePollOutcome::SlowDown { new_interval_secs }) => {
            if let Some(p) = state.pending_device_flow.write().await.as_mut() {
                p.interval_secs = new_interval_secs;
            }
            Ok(Json(DevicePollResponse::Pending))
        }
        Ok(oauth::DevicePollOutcome::Denied) => {
            *state.pending_device_flow.write().await = None;
            Ok(Json(DevicePollResponse::Denied))
        }
        Ok(oauth::DevicePollOutcome::Expired) => {
            *state.pending_device_flow.write().await = None;
            Ok(Json(DevicePollResponse::Expired))
        }
        Ok(oauth::DevicePollOutcome::Success(exchanged)) => {
            *state.pending_device_flow.write().await = None;
            persist_connection(&state, exchanged).await.map(Json)
        }
        Err(e) => {
            *state.pending_device_flow.write().await = None;
            Err(AppError::Internal(e))
        }
    }
}

async fn persist_connection(state: &AppState, exchanged: oauth::ExchangedToken) -> AppResult<DevicePollResponse> {
    let github_client = client::for_token(&exchanged.access_token).map_err(AppError::Internal)?;
    let login = discovery::validate_token(&github_client).await.map_err(AppError::Internal)?;
    let installation_id = discovery::find_installation_id(&github_client, GITHUB_APP_SLUG).await.map_err(AppError::Internal)?;

    let (token_encrypted, token_nonce) = state.enc.encrypt_str(&exchanged.access_token).map_err(AppError::Internal)?;
    let (refresh_encrypted, refresh_nonce) = state.enc.encrypt_str(&exchanged.refresh_token).map_err(AppError::Internal)?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(exchanged.expires_in)).to_rfc3339();

    token_queries::upsert_app_token(&state.db, &token_encrypted, &token_nonce, &refresh_encrypted, &refresh_nonce, &expires_at, installation_id, &login)
        .await?;
    client::invalidate(state).await;

    Ok(DevicePollResponse::Connected { github_login: login, has_installation: installation_id.is_some() })
}
