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
