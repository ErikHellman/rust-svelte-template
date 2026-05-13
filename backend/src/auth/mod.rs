pub mod invites;
pub mod jwt;
pub mod middleware;
pub mod oauth;
pub mod password;
pub mod session;

use crate::AppState;
use crate::error::{AppError, AppResult};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Duration, Utc};
use invites::Invite;
use middleware::AuthUser;
use oauth::{ExternalUser, Provider};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;
use url::form_urlencoded;

const STATE_COOKIE: &str = "oauth_state";
const REFRESH_COOKIE: &str = "refresh_token";

pub fn router() -> Router<AppState> {
    Router::new()
        // Login flows — only authenticate existing users.
        .route("/{provider}/start", get(oauth_login_start))
        .route("/{provider}/callback", get(oauth_callback))
        .route("/login", post(password_login))
        // Signup flows — invite-gated; create the user record.
        .route("/signup/invite/check", post(check_invite))
        .route("/signup/password", post(password_signup))
        .route("/{provider}/signup/start", get(oauth_signup_start))
        // Session management + introspection.
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

// -------------------------------------------------------------------------
// State cookie carried across the OAuth round-trip.
// -------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct OAuthStateCookie {
    provider: String,
    csrf: String,
    pkce: String,
    exp: i64,
    /// Present when this round-trip is a signup, absent for a login.
    invite_code: Option<String>,
}

fn build_state_cookie(state: &AppState, payload: &OAuthStateCookie) -> AppResult<Cookie<'static>> {
    let value = serde_json::to_string(payload)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encode state cookie: {e}")))?;
    Ok(Cookie::build((STATE_COOKIE, value))
        .http_only(true)
        .secure(!state.config.public_base_url.starts_with("http://localhost"))
        .same_site(SameSite::Lax)
        .path("/api/auth")
        .max_age(time::Duration::minutes(10))
        .build())
}

async fn oauth_login_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Redirect)> {
    let provider = Provider::from_str(&provider)?;
    let begin = state.oauth.start(provider)?;
    let payload = OAuthStateCookie {
        provider: provider.as_str().to_string(),
        csrf: begin.csrf_state,
        pkce: begin.pkce_verifier,
        exp: (Utc::now() + Duration::minutes(10)).timestamp(),
        invite_code: None,
    };
    let cookie = build_state_cookie(&state, &payload)?;
    Ok((jar.add(cookie), Redirect::to(&begin.authorize_url)))
}

#[derive(Deserialize)]
struct SignupStartQuery {
    code: String,
}

async fn oauth_signup_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<SignupStartQuery>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Redirect)> {
    let provider = Provider::from_str(&provider)?;
    // Cheap pre-check so a bad code fails before we redirect to the provider.
    let _invite = invites::find_valid(&state.db, &query.code)
        .await?
        .ok_or_else(|| AppError::BadRequest("invite code not found".into()))?;
    let begin = state.oauth.start(provider)?;
    let payload = OAuthStateCookie {
        provider: provider.as_str().to_string(),
        csrf: begin.csrf_state,
        pkce: begin.pkce_verifier,
        exp: (Utc::now() + Duration::minutes(10)).timestamp(),
        invite_code: Some(query.code),
    };
    let cookie = build_state_cookie(&state, &payload)?;
    Ok((jar.add(cookie), Redirect::to(&begin.authorize_url)))
}

#[derive(Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(params): Query<CallbackParams>,
    jar: CookieJar,
) -> AppResult<Response> {
    let provider = Provider::from_str(&provider)?;
    let cookie = jar
        .get(STATE_COOKIE)
        .ok_or(AppError::BadRequest("missing oauth state cookie".into()))?;
    let payload: OAuthStateCookie = serde_json::from_str(cookie.value())
        .map_err(|_| AppError::BadRequest("invalid oauth state cookie".into()))?;
    if payload.provider != provider.as_str() || payload.csrf != params.state {
        return Err(AppError::BadRequest("oauth state mismatch".into()));
    }
    if payload.exp < Utc::now().timestamp() {
        return Err(AppError::BadRequest("oauth state expired".into()));
    }

    let external = state
        .oauth
        .complete(provider, params.code, payload.pkce)
        .await?;

    // 1. Existing identity? Treat as login regardless of how they arrived.
    if let Some(user_id) = find_user_by_identity(&state, &external).await? {
        return finish_login_redirect(&state, jar, &user_id, "/#/notes").await;
    }

    // 2. No identity: this MUST be a signup with a valid invite.
    let Some(invite_code) = payload.invite_code else {
        return Ok(redirect_with_error(
            &state,
            jar,
            "/#/signup",
            "no account is linked to this provider yet — start from the signup page",
        ));
    };

    let user_id = match register_oauth_user(&state, &external, &invite_code).await {
        Ok(id) => id,
        Err(AppError::BadRequest(msg)) | Err(AppError::Conflict(msg)) => {
            return Ok(redirect_with_error(&state, jar, "/#/signup", &msg));
        }
        Err(other) => return Err(other),
    };

    finish_login_redirect(&state, jar, &user_id, "/#/notes").await
}

// -------------------------------------------------------------------------
// Signup helpers
// -------------------------------------------------------------------------

#[derive(Deserialize)]
struct CheckInviteBody {
    code: String,
}

#[derive(Serialize)]
struct CheckInviteResponse {
    valid: bool,
    /// If present, the signup form must use this email (read-only).
    bound_email: Option<String>,
    role: String,
}

async fn check_invite(
    State(state): State<AppState>,
    Json(body): Json<CheckInviteBody>,
) -> AppResult<Json<CheckInviteResponse>> {
    match invites::find_valid(&state.db, &body.code).await? {
        Some(invite) => Ok(Json(CheckInviteResponse {
            valid: true,
            bound_email: invite.email,
            role: invite.role,
        })),
        None => Ok(Json(CheckInviteResponse {
            valid: false,
            bound_email: None,
            role: "user".into(),
        })),
    }
}

#[derive(Deserialize)]
struct PasswordSignupBody {
    code: String,
    email: String,
    password: String,
    display_name: Option<String>,
}

async fn password_signup(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<PasswordSignupBody>,
) -> AppResult<Response> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(AppError::BadRequest("a valid email is required".into()));
    }
    password::validate_strength(&body.password)?;

    let invite = invites::find_valid(&state.db, &body.code)
        .await?
        .ok_or_else(|| AppError::BadRequest("invite code not found".into()))?;
    enforce_invite_email(&invite, &email)?;

    let password_hash = password::hash(&body.password)?;

    let mut tx = state.db.begin().await?;
    let user_id = create_user(
        &mut tx,
        &email,
        body.display_name.as_deref(),
        None,
        &invite.role,
    )
    .await?;
    password::insert(&mut tx, &user_id, &password_hash).await?;
    invites::mark_used(&mut tx, &invite.code, &user_id).await?;
    tx.commit().await?;

    finish_login_json(&state, jar, &user_id).await
}

async fn register_oauth_user(
    state: &AppState,
    ext: &ExternalUser,
    code: &str,
) -> AppResult<String> {
    let invite = invites::find_valid(&state.db, code)
        .await?
        .ok_or_else(|| AppError::BadRequest("invite code not found".into()))?;
    let email = ext.email.trim().to_lowercase();
    enforce_invite_email(&invite, &email)?;

    let mut tx = state.db.begin().await?;
    let user_id = create_user(
        &mut tx,
        &email,
        ext.display_name.as_deref(),
        ext.avatar_url.as_deref(),
        &invite.role,
    )
    .await?;
    let provider_str = ext.provider.as_str();
    sqlx::query!(
        "INSERT INTO identities (provider, provider_user_id, user_id) VALUES (?, ?, ?)",
        provider_str,
        ext.provider_user_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;
    invites::mark_used(&mut tx, &invite.code, &user_id).await?;
    tx.commit().await?;
    Ok(user_id)
}

fn enforce_invite_email(invite: &Invite, candidate: &str) -> AppResult<()> {
    if let Some(bound) = invite.email.as_deref() {
        if bound.trim().to_lowercase() != candidate.trim().to_lowercase() {
            return Err(AppError::BadRequest(
                "this invite is bound to a different email address".into(),
            ));
        }
    }
    Ok(())
}

async fn create_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    email: &str,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    role: &str,
) -> AppResult<String> {
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query!(
        "INSERT INTO users (id, email, display_name, avatar_url, role) VALUES (?, ?, ?, ?, ?)",
        id,
        email,
        display_name,
        avatar_url,
        role
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict("an account with that email already exists".into())
        }
        _ => AppError::Internal(anyhow::anyhow!("insert user: {e}")),
    })?;
    Ok(id)
}

async fn find_user_by_identity(state: &AppState, ext: &ExternalUser) -> AppResult<Option<String>> {
    let provider = ext.provider.as_str();
    let row = sqlx::query!(
        "SELECT user_id FROM identities WHERE provider = ? AND provider_user_id = ?",
        provider,
        ext.provider_user_id
    )
    .fetch_optional(&state.db)
    .await?;
    Ok(row.map(|r| r.user_id))
}

// -------------------------------------------------------------------------
// Password login
// -------------------------------------------------------------------------

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

async fn password_login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginBody>,
) -> AppResult<Response> {
    let email = body.email.trim().to_lowercase();
    let row = sqlx::query!(
        r#"SELECT u.id as "id!", u.role as "role!"
           FROM users u WHERE u.email = ?"#,
        email
    )
    .fetch_optional(&state.db)
    .await?;
    // Constant-ish error regardless of cause, to avoid leaking which emails
    // exist or which auth method they use.
    let unauthorized = || AppError::Unauthorized;

    let row = row.ok_or_else(unauthorized)?;
    let hash = password::find_hash(&state.db, &row.id)
        .await?
        .ok_or_else(unauthorized)?;
    password::verify(&hash, &body.password)?;

    finish_login_json(&state, jar, &row.id).await
}

// -------------------------------------------------------------------------
// Session minting + housekeeping
// -------------------------------------------------------------------------

/// Mint the access token + refresh cookie and return JSON. Used by the
/// password signup / login endpoints (which the SPA calls via fetch).
async fn finish_login_json(state: &AppState, jar: CookieJar, user_id: &str) -> AppResult<Response> {
    let (jar, access, role) = mint_session(state, jar, user_id).await?;
    Ok((
        jar,
        Json(LoginResponse {
            access_token: access,
            access_token_expires_in: state.config.access_token_ttl_secs,
            role,
        }),
    )
        .into_response())
}

/// Mint the access token + refresh cookie and 303 to the SPA. Used by the
/// OAuth callback, which arrives as a browser navigation.
async fn finish_login_redirect(
    state: &AppState,
    jar: CookieJar,
    user_id: &str,
    spa_hash_path: &str,
) -> AppResult<Response> {
    let (jar, _access, _role) = mint_session(state, jar, user_id).await?;
    let url = format!("{}{}", state.config.public_base_url, spa_hash_path);
    Ok((jar, Redirect::to(&url)).into_response())
}

async fn mint_session(
    state: &AppState,
    jar: CookieJar,
    user_id: &str,
) -> AppResult<(CookieJar, String, String)> {
    let role = lookup_role(state, user_id).await?;
    let access = state
        .jwt
        .mint_access(user_id, &role)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("mint access: {e}")))?;
    let refresh = session::issue(&state.db, user_id, state.config.refresh_token_ttl_secs).await?;
    let refresh_cookie = build_refresh_cookie(state, &refresh.cookie_value, refresh.expires_at);
    let jar = jar.remove(Cookie::from(STATE_COOKIE)).add(refresh_cookie);
    Ok((jar, access, role))
}

async fn lookup_role(state: &AppState, user_id: &str) -> AppResult<String> {
    let row = sqlx::query!(r#"SELECT role as "role!" FROM users WHERE id = ?"#, user_id)
        .fetch_one(&state.db)
        .await?;
    Ok(row.role)
}

fn redirect_with_error(state: &AppState, jar: CookieJar, path: &str, message: &str) -> Response {
    let encoded: String = form_urlencoded::byte_serialize(message.as_bytes()).collect();
    let url = format!("{}{}?error={}", state.config.public_base_url, path, encoded);
    let jar = jar.remove(Cookie::from(STATE_COOKIE));
    (jar, Redirect::to(&url)).into_response()
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<LoginResponse>)> {
    let cookie = jar.get(REFRESH_COOKIE).ok_or(AppError::Unauthorized)?;
    let rotated = session::rotate(
        &state.db,
        cookie.value(),
        state.config.refresh_token_ttl_secs,
    )
    .await?;
    let role = lookup_role(&state, &rotated.user_id).await?;
    let access = state
        .jwt
        .mint_access(&rotated.user_id, &role)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("mint access: {e}")))?;

    let new_cookie = build_refresh_cookie(&state, &rotated.cookie_value, rotated.expires_at);
    Ok((
        jar.add(new_cookie),
        Json(LoginResponse {
            access_token: access,
            access_token_expires_in: state.config.access_token_ttl_secs,
            role,
        }),
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, StatusCode)> {
    if let Some(cookie) = jar.get(REFRESH_COOKIE) {
        let _ = session::revoke(&state.db, cookie.value()).await;
    }
    let removal = Cookie::build((REFRESH_COOKIE, ""))
        .path("/api/auth")
        .max_age(time::Duration::seconds(0))
        .build();
    Ok((jar.add(removal), StatusCode::NO_CONTENT))
}

async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<MeResponse>> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", email as "email!", display_name, avatar_url, role as "role!"
           FROM users WHERE id = ?"#,
        user.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(MeResponse {
        id: row.id,
        email: row.email,
        display_name: row.display_name,
        avatar_url: row.avatar_url,
        role: row.role,
    }))
}

#[derive(Serialize)]
struct LoginResponse {
    access_token: String,
    access_token_expires_in: i64,
    role: String,
}

#[derive(Serialize)]
struct MeResponse {
    id: String,
    email: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    role: String,
}

fn build_refresh_cookie(
    state: &AppState,
    value: &str,
    expires_at: DateTime<Utc>,
) -> Cookie<'static> {
    let secure = !state.config.public_base_url.starts_with("http://localhost");
    let offset = OffsetDateTime::from_unix_timestamp(expires_at.timestamp()).unwrap_or_else(|_| {
        OffsetDateTime::now_utc() + time::Duration::seconds(state.config.refresh_token_ttl_secs)
    });
    Cookie::build((REFRESH_COOKIE, value.to_string()))
        .http_only(true)
        .secure(secure)
        .same_site(SameSite::Lax)
        .path("/api/auth")
        .expires(offset)
        .build()
}
