use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum_extra::extract::cookie::CookieJar;

use crate::app::AppState;
use crate::db::models::User;
use crate::db::queries::users as user_queries;

pub const SESSION_COOKIE: &str = "session";

/// Axum extractor: pulls the session cookie, validates the JWT + session record, and loads
/// the current `User`. Any handler that takes `CurrentUser` as a parameter is implicitly
/// auth-gated; routes that should stay public simply don't take it.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or((StatusCode::UNAUTHORIZED, "missing session cookie"))?;

        let claims = state
            .jwt
            .decode(&token)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid session token"))?;

        let valid = user_queries::session_valid(&state.db, &claims.sid)
            .await
            .unwrap_or(false);
        if !valid {
            return Err((StatusCode::UNAUTHORIZED, "session expired or revoked"));
        }

        let user = user_queries::find_by_id(&state.db, &claims.sub)
            .await
            .ok()
            .flatten()
            .ok_or((StatusCode::UNAUTHORIZED, "user not found"))?;

        Ok(CurrentUser(user))
    }
}

/// Extractor for routes that need the caller to be both authenticated *and* approved by an
/// admin. A session now only proves "authenticated as GitHub user X," not "authorized to
/// use the app" -- a pending or restricted user still gets a session (see
/// `auth::handlers::login_poll`) so they can see their own status and log out, but every
/// data-bearing route should take `ApprovedUser` instead of bare `CurrentUser` so they get a
/// clear 403 rather than real data.
pub struct ApprovedUser(pub User);

impl FromRequestParts<AppState> for ApprovedUser {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        if user.status != "approved" {
            return Err((StatusCode::FORBIDDEN, "account is not approved"));
        }
        Ok(ApprovedUser(user))
    }
}

/// Extractor for admin-only routes (whitelist management, approve/restrict, login events,
/// role changes). Requires `ApprovedUser` first, then the `admin` role on top of that.
pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let ApprovedUser(user) = ApprovedUser::from_request_parts(parts, state).await?;
        if user.role != "admin" {
            return Err((StatusCode::FORBIDDEN, "admin access required"));
        }
        Ok(AdminUser(user))
    }
}
