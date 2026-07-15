use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::middleware::{CurrentUser, SESSION_COOKIE};
use crate::auth::password;
use crate::db::queries::users as user_queries;
use crate::error::{AppError, AppResult};

const SESSION_TTL_DAYS: i64 = 30;

#[derive(Serialize)]
pub struct AuthStatus {
    pub needs_setup: bool,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<AuthStatus>> {
    let count = user_queries::count(&state.db).await?;
    Ok(Json(AuthStatus { needs_setup: count == 0 }))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub id: String,
    pub username: String,
    pub role: String,
}

pub async fn setup(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetupRequest>,
) -> AppResult<(CookieJar, Json<MeResponse>)> {
    let count = user_queries::count(&state.db).await?;
    if count > 0 {
        return Err(AppError::Conflict("an admin account already exists".into()));
    }
    if req.username.trim().len() < 3 || req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "username must be >=3 chars and password >=8 chars".into(),
        ));
    }

    let hash = password::hash(&req.password).map_err(AppError::Internal)?;
    let user = user_queries::create(&state.db, req.username.trim(), &hash, "admin").await?;
    let (jar, resp) = issue_session(&state, jar, &user).await?;
    Ok((jar, Json(resp)))
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<MeResponse>)> {
    let user = user_queries::find_by_username(&state.db, &req.username)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !password::verify(&req.password, &user.password_hash) {
        return Err(AppError::Unauthorized);
    }

    let (jar, resp) = issue_session(&state, jar, &user).await?;
    Ok((jar, Json(resp)))
}

async fn issue_session(
    state: &AppState,
    jar: CookieJar,
    user: &crate::db::models::User,
) -> AppResult<(CookieJar, MeResponse)> {
    let session_id = user_queries::create_session(&state.db, &user.id, chrono::Duration::days(SESSION_TTL_DAYS))
        .await?;
    let token = state
        .jwt
        .encode(&user.id, &session_id, chrono::Duration::days(SESSION_TTL_DAYS))
        .map_err(AppError::Internal)?;

    let cookie = Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::days(SESSION_TTL_DAYS))
        .build();

    Ok((
        jar.add(cookie),
        MeResponse {
            id: user.id.clone(),
            username: user.username.clone(),
            role: user.role.clone(),
        },
    ))
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

pub async fn me(CurrentUser(user): CurrentUser) -> Json<MeResponse> {
    Json(MeResponse {
        id: user.id,
        username: user.username,
        role: user.role,
    })
}

pub async fn list_users(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Vec<MeResponse>>> {
    let users = user_queries::list(&state.db).await?;
    Ok(Json(
        users
            .into_iter()
            .map(|u| MeResponse { id: u.id, username: u.username, role: u.role })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: Option<String>,
}

pub async fn create_user(
    State(state): State<AppState>,
    CurrentUser(actor): CurrentUser,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<Json<MeResponse>> {
    if actor.role != "admin" {
        return Err(AppError::Forbidden);
    }
    if req.username.trim().len() < 3 || req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "username must be >=3 chars and password >=8 chars".into(),
        ));
    }
    let hash = password::hash(&req.password).map_err(AppError::Internal)?;
    let user = user_queries::create(&state.db, req.username.trim(), &hash, req.role.as_deref().unwrap_or("member"))
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
                AppError::Conflict("username already taken".into())
            }
            other => AppError::Database(other),
        })?;
    Ok(Json(MeResponse { id: user.id, username: user.username, role: user.role }))
}

pub async fn delete_user(
    State(state): State<AppState>,
    CurrentUser(actor): CurrentUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> AppResult<()> {
    if actor.role != "admin" {
        return Err(AppError::Forbidden);
    }
    if actor.id == id {
        return Err(AppError::BadRequest("cannot delete your own account".into()));
    }
    user_queries::delete(&state.db, &id).await?;
    Ok(())
}
