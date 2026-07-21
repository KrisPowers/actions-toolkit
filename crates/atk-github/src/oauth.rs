use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

/// GitHub's OAuth token endpoint, used for both the device-flow poll and refresh grants.
/// Default value of `AppConfig::github_oauth_token_url`; tests override that config field to
/// point at a mock server instead of hardcoding around this constant.
pub const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// GitHub's device-code endpoint. Default value of `AppConfig::github_device_code_url`.
pub const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";

/// A pending device-flow connect attempt: the device code needed to poll for completion, and
/// when it expires. There is at most one of these at a time, held in
/// `AppStateInner::pending_device_flow`, since only one operator can be mid-connect on a
/// single-instance tool; starting a new attempt simply replaces whatever was there before.
#[derive(Clone)]
pub struct PendingDeviceFlow {
    pub device_code: String,
    pub interval_secs: i64,
    pub expires_at: DateTime<Utc>,
}

pub struct DeviceCodeStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
}

/// Starts a device-flow connect attempt: GitHub returns a `user_code` to show the operator and a
/// `device_code` this instance polls with. No client secret is sent (device flow is the one
/// GitHub OAuth flow that genuinely doesn't need one, see the doc comment on `refresh_access_token`
/// for why the redirect-based authorization-code flow this replaced needed one after all).
pub async fn start_device_flow(device_code_url: &str, client_id: &str) -> Result<DeviceCodeStart> {
    let client = reqwest::Client::new();
    let resp: DeviceCodeResponse = client
        .post(device_code_url)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id)])
        .send()
        .await
        .context("failed to reach GitHub's device code endpoint")?
        .json()
        .await
        .context("failed to parse GitHub's device code response")?;

    Ok(DeviceCodeStart {
        device_code: resp.device_code,
        user_code: resp.user_code,
        verification_uri: resp.verification_uri,
        expires_in: resp.expires_in,
        interval: resp.interval,
    })
}

pub struct ExchangedToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<i64>,
}

pub enum DevicePollOutcome {
    /// The operator hasn't approved (or denied) yet; keep polling at the current interval.
    Pending,
    /// GitHub asked for a longer interval between polls; the caller should use this from now on.
    SlowDown { new_interval_secs: i64 },
    /// The operator explicitly declined on GitHub's side.
    Denied,
    /// The device code expired before it was approved; the whole attempt has to restart.
    Expired,
    Success(ExchangedToken),
}

/// Polls once for whether a device-flow attempt has been approved yet. GitHub reports "not yet"
/// as a 200 with `error: "authorization_pending"` (not a non-2xx status), same shape as the
/// terminal failure/success cases, so all of it is read from the response body.
pub async fn poll_device_token(token_url: &str, client_id: &str, device_code: &str) -> Result<DevicePollOutcome> {
    let client = reqwest::Client::new();
    let resp: TokenResponse = client
        .post(token_url)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("device_code", device_code), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")])
        .send()
        .await
        .context("failed to reach GitHub's token endpoint")?
        .json()
        .await
        .context("failed to parse GitHub's token response")?;

    if let Some(err) = resp.error.as_deref() {
        return Ok(match err {
            "authorization_pending" => DevicePollOutcome::Pending,
            "slow_down" => DevicePollOutcome::SlowDown { new_interval_secs: resp.interval.unwrap_or(5) },
            "expired_token" => DevicePollOutcome::Expired,
            "access_denied" => DevicePollOutcome::Denied,
            other => anyhow::bail!("GitHub rejected the device-flow poll: {other} ({})", resp.error_description.unwrap_or_default()),
        });
    }

    let access_token = resp.access_token.context("GitHub's token response had no access_token")?;
    let refresh_token = resp
        .refresh_token
        .context("GitHub's token response had no refresh_token; is 'Expire user authorization tokens' enabled on the App?")?;
    let expires_in = resp
        .expires_in
        .context("GitHub's token response had no expires_in; is 'Expire user authorization tokens' enabled on the App?")?;
    Ok(DevicePollOutcome::Success(ExchangedToken { access_token, refresh_token, expires_in }))
}

/// Refreshes an expiring GitHub App user-to-server access token using the stored refresh token.
/// Unlike the initial device-flow exchange, GitHub's refresh grant genuinely takes no client
/// secret for any flow (device flow included), so this stays a plain `client_id` + refresh token
/// request.
pub async fn refresh_access_token(token_url: &str, client_id: &str, refresh_token: &str) -> Result<ExchangedToken> {
    let client = reqwest::Client::new();
    let resp: TokenResponse = client
        .post(token_url)
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("grant_type", "refresh_token"), ("refresh_token", refresh_token)])
        .send()
        .await
        .context("failed to reach GitHub's token endpoint")?
        .json()
        .await
        .context("failed to parse GitHub's token response")?;

    if let Some(err) = resp.error {
        anyhow::bail!("GitHub rejected the refresh: {err} ({})", resp.error_description.unwrap_or_default());
    }
    let access_token = resp.access_token.context("GitHub's token response had no access_token")?;
    let refresh_token = resp.refresh_token.context("GitHub's token response had no refresh_token")?;
    let expires_in = resp.expires_in.context("GitHub's token response had no expires_in")?;
    Ok(ExchangedToken { access_token, refresh_token, expires_in })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn start_device_flow_parses_the_response() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "d-123",
                "user_code": "ABCD-1234",
                "verification_uri": "https://github.com/login/device",
                "expires_in": 900,
                "interval": 5
            })))
            .mount(&mock_server)
            .await;

        let started = start_device_flow(&mock_server.uri(), "client-id").await.unwrap();
        assert_eq!(started.device_code, "d-123");
        assert_eq!(started.user_code, "ABCD-1234");
        assert_eq!(started.interval, 5);
    }

    #[tokio::test]
    async fn poll_reports_pending_while_unapproved() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "authorization_pending" })))
            .mount(&mock_server)
            .await;

        let outcome = poll_device_token(&mock_server.uri(), "client-id", "d-123").await.unwrap();
        assert!(matches!(outcome, DevicePollOutcome::Pending));
    }

    #[tokio::test]
    async fn poll_reports_denied_when_the_operator_declines() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "access_denied" })))
            .mount(&mock_server)
            .await;

        let outcome = poll_device_token(&mock_server.uri(), "client-id", "d-123").await.unwrap();
        assert!(matches!(outcome, DevicePollOutcome::Denied));
    }

    #[tokio::test]
    async fn poll_reports_expired_when_the_device_code_times_out() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "expired_token" })))
            .mount(&mock_server)
            .await;

        let outcome = poll_device_token(&mock_server.uri(), "client-id", "d-123").await.unwrap();
        assert!(matches!(outcome, DevicePollOutcome::Expired));
    }

    #[tokio::test]
    async fn poll_reports_slow_down_with_the_new_interval() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "error": "slow_down", "interval": 10 })))
            .mount(&mock_server)
            .await;

        let outcome = poll_device_token(&mock_server.uri(), "client-id", "d-123").await.unwrap();
        assert!(matches!(outcome, DevicePollOutcome::SlowDown { new_interval_secs: 10 }));
    }

    #[tokio::test]
    async fn poll_reports_success_with_the_issued_tokens() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ghu_abc",
                "refresh_token": "ghr_def",
                "expires_in": 28800
            })))
            .mount(&mock_server)
            .await;

        let outcome = poll_device_token(&mock_server.uri(), "client-id", "d-123").await.unwrap();
        match outcome {
            DevicePollOutcome::Success(t) => {
                assert_eq!(t.access_token, "ghu_abc");
                assert_eq!(t.refresh_token, "ghr_def");
                assert_eq!(t.expires_in, 28800);
            }
            _ => panic!("expected Success"),
        }
    }
}
