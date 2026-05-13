//! Email + password credentials. Hashes are stored in `password_credentials`,
//! one row per user (separate from `users` so passwords can be added / removed
//! / rotated independently).

use crate::error::{AppError, AppResult};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use sqlx::{Sqlite, Transaction};

const MIN_PASSWORD_LEN: usize = 8;

pub fn validate_strength(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

pub fn hash(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("argon2 hash failed: {e}")))?
        .to_string();
    Ok(hash)
}

pub fn verify(stored: &str, password: &str) -> AppResult<()> {
    let parsed = PasswordHash::new(stored)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored hash: {e}")))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| AppError::Unauthorized)?;
    Ok(())
}

pub async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    user_id: &str,
    password_hash: &str,
) -> AppResult<()> {
    sqlx::query!(
        "INSERT INTO password_credentials (user_id, password_hash) VALUES (?, ?)",
        user_id,
        password_hash
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn find_hash(pool: &sqlx::SqlitePool, user_id: &str) -> AppResult<Option<String>> {
    let row = sqlx::query!(
        r#"SELECT password_hash as "password_hash!" FROM password_credentials WHERE user_id = ?"#,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.password_hash))
}
