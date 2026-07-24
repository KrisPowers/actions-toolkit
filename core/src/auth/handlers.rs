use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::login_flow::{LoginFlowResult, LoginFlowState};
use crate::auth::middleware::{AdminUser, ApprovedUser, CurrentUser, SESSION_COOKIE};
use crate::db::models::User;
use crate::db::queries::{github_token as token_queries, login_events as login_event_queries, users as user_queries, whitelist as whitelist_queries};
use crate::error::{AppError, AppResult};
use crate::github::{client, discovery, oauth};

const SESSION_TTL_DAYS: i64 = 30;

#[derive(Serialize)]
pub struct AuthStatus {
    /// True until both an admin account and a GitHub token exist; the setup wizard stays up
    /// on the frontend for as long as this is true.
    pub needs_setup: bool,
    pub needs_admin: bool,
    pub needs_github_token: bool,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<AuthStatus>> {
    let user_count = user_queries::count(&state.db).await?;
    let has_token = token_queries::get(&state.db).await?.is_some();
    let needs_admin = user_count == 0;
    let needs_github_token = !has_token;
    Ok(Json(AuthStatus {
        needs_setup: needs_admin || needs_github_token,
        needs_admin,
        needs_github_token,
    }))
}

#[derive(Serialize)]
pub struct MeResponse {
    pub id: String,
    pub github_login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: String,
    pub status: String,
}

impl From<User> for MeResponse {
    fn from(u: User) -> Self {
        MeResponse { id: u.id, github_login: u.github_login, display_name: u.display_name, avatar_url: u.avatar_url, role: u.role, status: u.status }
    }
}

/// Deliberately takes `CurrentUser`, not `ApprovedUser`: a pending or restricted account
/// still needs to see its own status (and log out), just not any app data.
pub async fn me(CurrentUser(user): CurrentUser) -> Json<MeResponse> {
    Json(user.into())
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> AppResult<impl IntoResponse> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        if let Ok(claims) = state.jwt.decode(cookie.value()) {
            let _ = user_queries::revoke_session(&state.db, &claims.sid).await;
        }
    }
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    Ok(jar)
}

pub async fn list_users(State(state): State<AppState>, _user: ApprovedUser) -> AppResult<Json<Vec<MeResponse>>> {
    let users = user_queries::list(&state.db).await?;
    Ok(Json(users.into_iter().map(MeResponse::from).collect()))
}

async fn issue_session(state: &AppState, jar: CookieJar, user: &User) -> AppResult<(CookieJar, MeResponse)> {
    let session_id = user_queries::create_session(&state.db, &user.id, chrono::Duration::days(SESSION_TTL_DAYS)).await?;
    let token = state.jwt.encode(&user.id, &session_id, chrono::Duration::days(SESSION_TTL_DAYS)).map_err(AppError::Internal)?;

    let cookie = Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::days(SESSION_TTL_DAYS))
        .build();

    Ok((jar.add(cookie), user.clone().into()))
}

#[derive(Serialize)]
pub struct LoginStartResponse {
    pub attempt_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: i64,
    pub expires_in: i64,
}

/// Starts a GitHub-login device-flow attempt. Public (no `CurrentUser`) since this is how a
/// person becomes a user in the first place. Rate limited per client IP: unlike the
/// account-wide repo-access connect flow (admin-only, rare), this endpoint is reachable by
/// anyone who can see the login page.
pub async fn login_start(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> AppResult<Json<LoginStartResponse>> {
    let ip = crate::net::client_ip(&headers, addr);
    let user_agent = headers.get(axum::http::header::USER_AGENT).and_then(|v| v.to_str().ok()).map(str::to_string);

    if !state.login_rate_limiter.check(&ip) {
        login_event_queries::record(&state.db, None, None, None, Some(&ip), user_agent.as_deref(), "rate_limited").await?;
        return Err(AppError::RateLimited("too many login attempts, please wait a while and try again".into()));
    }

    sweep_expired_login_flows(&state).await;

    let started = oauth::start_device_flow(&state.config.github_device_code_url, &state.config.github_app_client_id).await.map_err(AppError::Internal)?;

    let attempt_id = Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(started.expires_in);
    state.login_flows.write().await.insert(
        attempt_id.clone(),
        LoginFlowState { device_code: started.device_code.clone(), interval_secs: started.interval, expires_at, result: None },
    );

    tokio::spawn(run_login_flow_poller(state.clone(), attempt_id.clone(), started.device_code.clone(), started.interval, ip, user_agent));

    Ok(Json(LoginStartResponse {
        attempt_id,
        user_code: started.user_code,
        verification_uri: started.verification_uri,
        interval: started.interval,
        expires_in: started.expires_in,
    }))
}

/// Removes attempts whose device code has long since expired and were never polled to
/// completion (e.g. the browser tab was closed). Run opportunistically from `login_start`
/// rather than on a background timer, since the key set only ever grows from real login
/// attempts on a self-hosted, single-instance tool.
async fn sweep_expired_login_flows(state: &AppState) {
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(10);
    state.login_flows.write().await.retain(|_, flow| flow.expires_at > cutoff);
}

/// Polls GitHub for this login attempt's outcome until a terminal one, independent of
/// whether the frontend is still polling `login_poll` -- same reasoning as
/// `github_oauth::run_device_flow_poller`. Each attempt owns its own map entry (keyed by
/// `attempt_id`), so unlike that single-slot flow there's no "did a newer attempt supersede
/// this one" check needed here.
async fn run_login_flow_poller(state: AppState, attempt_id: String, device_code: String, mut interval_secs: i64, ip: String, user_agent: Option<String>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs.max(1) as u64)).await;

        let expires_at = match state.login_flows.read().await.get(&attempt_id) {
            Some(flow) => flow.expires_at,
            None => return,
        };
        if chrono::Utc::now() > expires_at {
            finish_login_flow(&state, &attempt_id, LoginFlowResult::Expired).await;
            return;
        }

        match oauth::poll_device_token(&state.config.github_oauth_token_url, &state.config.github_app_client_id, &device_code).await {
            Ok(oauth::DevicePollOutcome::Pending) => continue,
            Ok(oauth::DevicePollOutcome::SlowDown { new_interval_secs }) => {
                interval_secs = new_interval_secs;
                if let Some(flow) = state.login_flows.write().await.get_mut(&attempt_id) {
                    flow.interval_secs = new_interval_secs;
                }
            }
            Ok(oauth::DevicePollOutcome::Denied) => {
                let _ = login_event_queries::record(&state.db, None, None, None, Some(&ip), user_agent.as_deref(), "denied").await;
                finish_login_flow(&state, &attempt_id, LoginFlowResult::Denied).await;
                return;
            }
            Ok(oauth::DevicePollOutcome::Expired) => {
                finish_login_flow(&state, &attempt_id, LoginFlowResult::Expired).await;
                return;
            }
            Ok(oauth::DevicePollOutcome::Success(exchanged)) => {
                let result = match resolve_login(&state, exchanged, &ip, user_agent.as_deref()).await {
                    Ok(user) => LoginFlowResult::Authenticated(user),
                    Err(e) => {
                        tracing::warn!(error = %e, "github login succeeded but resolving the account failed");
                        LoginFlowResult::Failed { message: e.to_string() }
                    }
                };
                finish_login_flow(&state, &attempt_id, result).await;
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, "login device-flow poll request to GitHub failed");
                finish_login_flow(&state, &attempt_id, LoginFlowResult::Failed { message: e.to_string() }).await;
                return;
            }
        }
    }
}

async fn finish_login_flow(state: &AppState, attempt_id: &str, result: LoginFlowResult) {
    if let Some(flow) = state.login_flows.write().await.get_mut(attempt_id) {
        flow.result = Some(result);
    }
}

/// Resolves an exchanged device-flow token into a `users` row: fetches the GitHub identity,
/// decides the default role/status for a brand new account (first-ever user becomes admin,
/// a whitelisted login is auto-approved, anything else starts pending), upserts the row, and
/// records the login event unconditionally.
async fn resolve_login(state: &AppState, exchanged: oauth::ExchangedToken, ip: &str, user_agent: Option<&str>) -> anyhow::Result<User> {
    let github_client = client::for_token(&exchanged.access_token)?;
    let identity = discovery::fetch_identity(&github_client).await?;

    let is_first_user = user_queries::count(&state.db).await? == 0;
    let already_exists = user_queries::find_by_github_id(&state.db, identity.id).await?.is_some();
    let (default_role, default_status) = if is_first_user {
        ("admin", "approved")
    } else if whitelist_queries::is_whitelisted(&state.db, &identity.login).await? {
        ("member", "approved")
    } else {
        ("member", "pending")
    };

    let user = user_queries::upsert_from_github(
        &state.db,
        identity.id,
        &identity.login,
        identity.name.as_deref(),
        identity.avatar_url.as_deref(),
        default_role,
        default_status,
    )
    .await?;

    // A returning user keeps whatever status an admin has since set (upsert_from_github only
    // applies default_status on first insert), so the recorded outcome reflects the account's
    // actual current status, not the defaults computed above.
    let outcome = if already_exists { user.status.clone() } else { default_status.to_string() };
    login_event_queries::record(&state.db, Some(&user.id), Some(&identity.login), Some(identity.id), Some(ip), user_agent, &outcome).await?;

    Ok(user)
}

#[derive(Deserialize)]
pub struct LoginPollRequest {
    pub attempt_id: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginPollResponse {
    Pending,
    Denied,
    Expired,
    /// No attempt is in progress under this `attempt_id` (never started, already consumed by
    /// a prior poll, or swept for having long expired unpolled).
    NotStarted,
    Approved { user: MeResponse },
    PendingApproval { user: MeResponse },
    Restricted { user: MeResponse },
}

pub async fn login_poll(State(state): State<AppState>, jar: CookieJar, Json(req): Json<LoginPollRequest>) -> AppResult<(CookieJar, Json<LoginPollResponse>)> {
    let result = state.login_flows.read().await.get(&req.attempt_id).and_then(|f| f.result.clone());

    let Some(result) = result else {
        let is_pending = state.login_flows.read().await.contains_key(&req.attempt_id);
        return Ok((jar, Json(if is_pending { LoginPollResponse::Pending } else { LoginPollResponse::NotStarted })));
    };

    // The attempt is resolved; drop it so the map doesn't hold finished attempts forever.
    state.login_flows.write().await.remove(&req.attempt_id);

    match result {
        LoginFlowResult::Denied => Ok((jar, Json(LoginPollResponse::Denied))),
        LoginFlowResult::Expired => Ok((jar, Json(LoginPollResponse::Expired))),
        LoginFlowResult::Failed { message } => Err(AppError::Internal(anyhow::anyhow!(message))),
        LoginFlowResult::Authenticated(user) => {
            let status = user.status.clone();
            let (jar, resp) = issue_session(&state, jar, &user).await?;
            let response = match status.as_str() {
                "approved" => LoginPollResponse::Approved { user: resp },
                "restricted" => LoginPollResponse::Restricted { user: resp },
                _ => LoginPollResponse::PendingApproval { user: resp },
            };
            Ok((jar, Json(response)))
        }
    }
}

const VALID_STATUSES: [&str; 3] = ["approved", "restricted", "pending"];
const VALID_ROLES: [&str; 2] = ["admin", "member"];

#[derive(Deserialize)]
pub struct SetStatusRequest {
    pub status: String,
}

/// Approves, restricts, or resets a user's access. Admin only: this is the gate between
/// "authenticated via GitHub" and "can see app data" (see `ApprovedUser`).
pub async fn set_user_status(
    State(state): State<AppState>,
    AdminUser(_actor): AdminUser,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<SetStatusRequest>,
) -> AppResult<()> {
    if !VALID_STATUSES.contains(&req.status.as_str()) {
        return Err(AppError::BadRequest(format!("status must be one of {}", VALID_STATUSES.join(", "))));
    }
    user_queries::set_status(&state.db, &id, &req.status).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct SetRoleRequest {
    pub role: String,
}

pub async fn set_user_role(
    State(state): State<AppState>,
    AdminUser(actor): AdminUser,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<SetRoleRequest>,
) -> AppResult<()> {
    if !VALID_ROLES.contains(&req.role.as_str()) {
        return Err(AppError::BadRequest(format!("role must be one of {}", VALID_ROLES.join(", "))));
    }
    if actor.id == id && req.role != "admin" {
        return Err(AppError::BadRequest("cannot demote your own account".into()));
    }
    user_queries::set_role(&state.db, &id, &req.role).await?;
    Ok(())
}

pub async fn delete_user(State(state): State<AppState>, AdminUser(actor): AdminUser, axum::extract::Path(id): axum::extract::Path<String>) -> AppResult<()> {
    if actor.id == id {
        return Err(AppError::BadRequest("cannot delete your own account".into()));
    }
    user_queries::delete(&state.db, &id).await?;
    Ok(())
}

pub async fn list_whitelist(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<Vec<crate::db::models::WhitelistEntry>>> {
    Ok(Json(whitelist_queries::list(&state.db).await?))
}

#[derive(Deserialize)]
pub struct AddWhitelistRequest {
    pub github_login: String,
}

/// Pre-approves a GitHub login before that person has ever signed in; see
/// `whitelist_queries::add`'s doc comment for why this is a separate table from `users`.
pub async fn add_whitelist(State(state): State<AppState>, AdminUser(actor): AdminUser, Json(req): Json<AddWhitelistRequest>) -> AppResult<()> {
    let login = req.github_login.trim();
    if login.is_empty() {
        return Err(AppError::BadRequest("github_login is required".into()));
    }
    whitelist_queries::add(&state.db, login, Some(&actor.id)).await?;
    Ok(())
}

pub async fn remove_whitelist(State(state): State<AppState>, _admin: AdminUser, axum::extract::Path(login): axum::extract::Path<String>) -> AppResult<()> {
    whitelist_queries::remove(&state.db, &login).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ListLoginEventsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_login_events(
    State(state): State<AppState>,
    _admin: AdminUser,
    axum::extract::Query(q): axum::extract::Query<ListLoginEventsQuery>,
) -> AppResult<Json<Vec<crate::db::models::LoginEvent>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    Ok(Json(login_event_queries::list(&state.db, limit, offset).await?))
}
