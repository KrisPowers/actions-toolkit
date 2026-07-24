/// Login attempts per client IP allowed within `LOGIN_RATE_LIMIT_WINDOW` before
/// `/auth/github/login/start` starts rejecting with a rate-limited outcome.
pub const LOGIN_RATE_LIMIT_MAX_ATTEMPTS: u32 = 10;
pub const LOGIN_RATE_LIMIT_WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// State for an in-flight GitHub-login device-flow attempt, keyed by a server-generated
/// `attempt_id` in `AppStateInner::login_flows`.
///
/// Unlike the account-wide repo-access connect flow (`AppStateInner::pending_device_flow`),
/// which is a single shared slot because only one operator is ever mid-connect on a
/// single-instance tool, login attempts are keyed: multiple different people can be
/// mid-login at the same time, so each attempt gets its own entry instead of clobbering
/// whichever one was already in progress.
pub struct LoginFlowState {
    pub device_code: String,
    pub interval_secs: i64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub result: Option<LoginFlowResult>,
}

/// Terminal outcome of a login attempt. Carries the resolved `User` (whatever their
/// `status`) rather than just "the device code was approved," since `login_poll` needs
/// enough to issue a session and report the account's approval state in one step.
#[derive(Clone)]
pub enum LoginFlowResult {
    Denied,
    Expired,
    Authenticated(crate::db::models::User),
    Failed { message: String },
}
