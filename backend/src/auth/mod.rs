pub mod jwt;
pub mod middleware;
pub mod oauth;
pub mod session;

use crate::AppState;
use crate::error::{AppError, AppResult};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, Duration, Utc};
use middleware::AuthUser;
use oauth::{ExternalUser, Provider};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use time::OffsetDateTime;

const STATE_COOKIE: &str = "oauth_state";
const REFRESH_COOKIE: &str = "refresh_token";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{provider}/start", get(start))
        .route("/{provider}/callback", get(callback))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Serialize, Deserialize)]
struct OAuthStateCookie {
    provider: String,
    csrf: String,
    pkce: String,
    exp: i64,
}

async fn start(
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
    };
    let value = serde_json::to_string(&payload)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encode state cookie: {e}")))?;
    let cookie = Cookie::build((STATE_COOKIE, value))
        .http_only(true)
        .secure(!state.config.public_base_url.starts_with("http://localhost"))
        .same_site(SameSite::Lax)
        .path("/api/auth")
        .max_age(time::Duration::minutes(10))
        .build();
    Ok((jar.add(cookie), Redirect::to(&begin.authorize_url)))
}

#[derive(Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

async fn callback(
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
    let user_id = upsert_user(&state, &external).await?;

    let refresh = session::issue(&state.db, &user_id, state.config.refresh_token_ttl_secs).await?;
    let refresh_cookie = build_refresh_cookie(&state, &refresh.cookie_value, refresh.expires_at);

    let jar = jar.remove(Cookie::from(STATE_COOKIE)).add(refresh_cookie);

    // Land the user on the SPA. The SPA will call /refresh on boot to get its
    // access token and pick up where they left off.
    Ok((
        jar,
        Redirect::to(&format!("{}/#/notes", state.config.public_base_url)),
    )
        .into_response())
}

async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(CookieJar, Json<RefreshResponse>)> {
    let cookie = jar.get(REFRESH_COOKIE).ok_or(AppError::Unauthorized)?;
    let rotated = session::rotate(
        &state.db,
        cookie.value(),
        state.config.refresh_token_ttl_secs,
    )
    .await?;
    let access = state
        .jwt
        .mint_access(&rotated.user_id)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("mint access: {e}")))?;

    let new_cookie = build_refresh_cookie(&state, &rotated.cookie_value, rotated.expires_at);
    Ok((
        jar.add(new_cookie),
        Json(RefreshResponse {
            access_token: access,
            access_token_expires_in: state.config.access_token_ttl_secs,
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

async fn me(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> AppResult<Json<MeResponse>> {
    let row = sqlx::query!(
        r#"SELECT id as "id!", email as "email!", display_name, avatar_url
           FROM users WHERE id = ?"#,
        user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(Json(MeResponse {
        id: row.id,
        email: row.email,
        display_name: row.display_name,
        avatar_url: row.avatar_url,
    }))
}

#[derive(Serialize)]
struct RefreshResponse {
    access_token: String,
    access_token_expires_in: i64,
}

#[derive(Serialize)]
struct MeResponse {
    id: String,
    email: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
}

async fn upsert_user(state: &AppState, ext: &ExternalUser) -> AppResult<String> {
    let provider_str = ext.provider.as_str();

    if let Some(row) = sqlx::query!(
        "SELECT user_id FROM identities WHERE provider = ? AND provider_user_id = ?",
        provider_str,
        ext.provider_user_id
    )
    .fetch_optional(&state.db)
    .await?
    {
        return Ok(row.user_id);
    }

    // No identity yet. Always create a fresh user — account-linking across
    // providers by email is a project-specific policy decision. See CLAUDE.md.
    let mut tx = state.db.begin().await?;
    let user_id = uuid::Uuid::now_v7().to_string();
    sqlx::query!(
        "INSERT INTO users (id, email, display_name, avatar_url) VALUES (?, ?, ?, ?)",
        user_id,
        ext.email,
        ext.display_name,
        ext.avatar_url,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            AppError::Conflict("an account with that email already exists".into())
        }
        _ => AppError::Internal(anyhow::anyhow!("insert user: {e}")),
    })?;
    sqlx::query!(
        "INSERT INTO identities (provider, provider_user_id, user_id) VALUES (?, ?, ?)",
        provider_str,
        ext.provider_user_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(user_id)
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

// Headers helper retained for potential future use.
#[allow(dead_code)]
fn no_cache() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h
}
