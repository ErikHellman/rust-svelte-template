use crate::error::{AppError, AppResult};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sqlx::SqlitePool;

pub struct IssuedRefresh {
    pub cookie_value: String,
    pub expires_at: DateTime<Utc>,
}

pub struct RotatedRefresh {
    pub user_id: String,
    pub cookie_value: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn issue(pool: &SqlitePool, user_id: &str, ttl_secs: i64) -> AppResult<IssuedRefresh> {
    let id = uuid::Uuid::now_v7().to_string();
    let secret = random_secret();
    let token_hash = hash_secret(&secret)?;
    let expires_at = Utc::now() + Duration::seconds(ttl_secs);
    let expires_at_str = expires_at.to_rfc3339();

    sqlx::query!(
        "INSERT INTO refresh_tokens (id, token_hash, user_id, expires_at) VALUES (?, ?, ?, ?)",
        id,
        token_hash,
        user_id,
        expires_at_str,
    )
    .execute(pool)
    .await?;

    Ok(IssuedRefresh {
        cookie_value: format!("{id}.{secret}"),
        expires_at,
    })
}

pub async fn rotate(
    pool: &SqlitePool,
    cookie_value: &str,
    ttl_secs: i64,
) -> AppResult<RotatedRefresh> {
    let (id, secret) = parse_cookie(cookie_value)?;

    let row = sqlx::query!(
        "SELECT id, token_hash, user_id, expires_at, revoked_at FROM refresh_tokens WHERE id = ?",
        id
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Unauthorized)?;

    if row.revoked_at.is_some() {
        return Err(AppError::Unauthorized);
    }
    let expires_at: DateTime<Utc> = row.expires_at.parse().map_err(|_| AppError::Unauthorized)?;
    if expires_at < Utc::now() {
        return Err(AppError::Unauthorized);
    }
    verify_secret(&row.token_hash, &secret)?;

    let new = issue(pool, &row.user_id, ttl_secs).await?;
    let new_id = new
        .cookie_value
        .split_once('.')
        .map(|(id, _)| id.to_string())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("malformed new cookie")))?;
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), replaced_by = ? WHERE id = ?",
        new_id,
        id,
    )
    .execute(pool)
    .await?;

    Ok(RotatedRefresh {
        user_id: row.user_id,
        cookie_value: new.cookie_value,
        expires_at: new.expires_at,
    })
}

pub async fn revoke(pool: &SqlitePool, cookie_value: &str) -> AppResult<()> {
    let (id, _) = match parse_cookie(cookie_value) {
        Ok(parts) => parts,
        Err(_) => return Ok(()),
    };
    sqlx::query!(
        "UPDATE refresh_tokens SET revoked_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? AND revoked_at IS NULL",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn parse_cookie(value: &str) -> AppResult<(String, String)> {
    let (id, secret) = value.split_once('.').ok_or(AppError::Unauthorized)?;
    if id.is_empty() || secret.is_empty() {
        return Err(AppError::Unauthorized);
    }
    Ok((id.to_string(), secret.to_string()))
}

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_secret(secret: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("argon2 hash failed: {e}")))?
        .to_string();
    Ok(hash)
}

fn verify_secret(hash_str: &str, secret: &str) -> AppResult<()> {
    let parsed = PasswordHash::new(hash_str)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored hash: {e}")))?;
    Argon2::default()
        .verify_password(secret.as_bytes(), &parsed)
        .map_err(|_| AppError::Unauthorized)?;
    Ok(())
}
