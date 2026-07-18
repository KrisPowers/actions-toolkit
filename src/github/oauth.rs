use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const GITHUB_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// How long an unused authorize attempt (state + PKCE verifier) stays valid before a callback
/// using it is rejected as stale.
const PENDING_TTL_MINUTES: i64 = 5;

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

/// Generates a PKCE code verifier (32 random bytes, base64url-encoded to 43 characters, within
/// the RFC 7636 43-128 range) and its S256 challenge.
pub fn generate_pkce() -> Pkce {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    Pkce { verifier, challenge }
}

/// Generates an opaque, unguessable CSRF state value for the authorize request.
pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// A stored, not-yet-completed authorize attempt: the PKCE verifier the callback needs to
/// complete the exchange, and when it was issued. Keyed by the CSRF `state` value in
/// `AppStateInner::oauth_states`; removed the moment a callback looks it up, so a state value
/// can be redeemed at most once by construction.
pub struct PendingAuthorize {
    pub code_verifier: String,
    pub created_at: DateTime<Utc>,
}

impl PendingAuthorize {
    pub fn is_expired(&self) -> bool {
        Utc::now() - self.created_at > chrono::Duration::minutes(PENDING_TTL_MINUTES)
    }
}

/// Looks up and removes the pending authorize attempt for `state_param`. Removing on lookup
/// means a second call with the same state (a replay) finds nothing, and a `None` state param
/// (missing entirely) is rejected before any map access. Pure and side-effect-free beyond the
/// map mutation, so it's directly unit-testable without a running server or a GitHub call.
pub fn take_pending(
    states: &dashmap::DashMap<String, PendingAuthorize>,
    state_param: Option<&str>,
) -> Result<PendingAuthorize> {
    let key = state_param.ok_or_else(|| anyhow::anyhow!("missing state parameter"))?;
    let pending = states
        .remove(key)
        .map(|(_, v)| v)
        .ok_or_else(|| anyhow::anyhow!("unknown or already-used state parameter"))?;
    if pending.is_expired() {
        anyhow::bail!("expired state parameter");
    }
    Ok(pending)
}

pub fn authorize_url(client_id: &str, redirect_uri: &str, state: &str, challenge: &str) -> String {
    let mut url = reqwest::Url::parse(GITHUB_AUTHORIZE_URL).expect("GITHUB_AUTHORIZE_URL is a valid static URL");
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    url.to_string()
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
}

/// Exchanges an authorization `code` and its PKCE `code_verifier` for an access token. No
/// client secret is sent: the App is a public OAuth client and PKCE is the only proof required.
/// GitHub reports failures (denied, expired code) as a 200 with an `error` field rather than a
/// non-2xx status, so that's checked explicitly rather than relying on HTTP status alone.
pub async fn exchange_code(client_id: &str, code: &str, code_verifier: &str, redirect_uri: &str) -> Result<ExchangedToken> {
    let client = reqwest::Client::new();
    let resp: TokenResponse = client
        .post(GITHUB_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", code_verifier),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .context("failed to reach GitHub's token endpoint")?
        .json()
        .await
        .context("failed to parse GitHub's token response")?;

    if let Some(err) = resp.error {
        anyhow::bail!("GitHub rejected the code exchange: {err} ({})", resp.error_description.unwrap_or_default());
    }

    let access_token = resp.access_token.context("GitHub's token response had no access_token")?;
    let refresh_token = resp
        .refresh_token
        .context("GitHub's token response had no refresh_token; is 'Expire user authorization tokens' enabled on the App?")?;
    let expires_in = resp
        .expires_in
        .context("GitHub's token response had no expires_in; is 'Expire user authorization tokens' enabled on the App?")?;

    Ok(ExchangedToken { access_token, refresh_token, expires_in })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashmap::DashMap;

    #[test]
    fn pkce_challenge_is_the_sha256_of_the_verifier() {
        let pkce = generate_pkce();
        assert!(pkce.verifier.len() >= 43 && pkce.verifier.len() <= 128);
        assert!(pkce.verifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));

        let mut hasher = Sha256::new();
        hasher.update(pkce.verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(pkce.challenge, expected);
    }

    #[test]
    fn missing_state_is_rejected() {
        let states: DashMap<String, PendingAuthorize> = DashMap::new();
        assert!(take_pending(&states, None).is_err());
    }

    #[test]
    fn unknown_state_is_rejected() {
        let states: DashMap<String, PendingAuthorize> = DashMap::new();
        assert!(take_pending(&states, Some("never-issued")).is_err());
    }

    #[test]
    fn reused_state_is_rejected_on_the_second_use() {
        let states: DashMap<String, PendingAuthorize> = DashMap::new();
        states.insert("s1".to_string(), PendingAuthorize { code_verifier: "v".to_string(), created_at: Utc::now() });

        assert!(take_pending(&states, Some("s1")).is_ok());
        assert!(take_pending(&states, Some("s1")).is_err());
    }

    #[test]
    fn expired_state_is_rejected() {
        let states: DashMap<String, PendingAuthorize> = DashMap::new();
        states.insert(
            "s1".to_string(),
            PendingAuthorize { code_verifier: "v".to_string(), created_at: Utc::now() - chrono::Duration::minutes(PENDING_TTL_MINUTES + 1) },
        );
        assert!(take_pending(&states, Some("s1")).is_err());
    }
}
