//! Single-use invite codes used to gate all user registration.

use crate::error::{AppError, AppResult};
use chrono::Utc;
use serde::Serialize;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, Serialize)]
pub struct Invite {
    pub code: String,
    pub email: Option<String>,
    pub role: String,
}

/// Look up an unused, unexpired invite. Returns `None` if the code doesn't
/// exist; returns `Err(AppError::BadRequest)` if it does but has been used or
/// has expired (we surface these to the user since they likely typed a real
/// code that's just no longer valid).
pub async fn find_valid(pool: &SqlitePool, code: &str) -> AppResult<Option<Invite>> {
    let row = sqlx::query!(
        r#"SELECT code as "code!", email, role as "role!", expires_at, used_at
           FROM invite_codes WHERE code = ?"#,
        code
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    if row.used_at.is_some() {
        return Err(AppError::BadRequest(
            "invite code has already been used".into(),
        ));
    }
    if let Some(exp) = row.expires_at.as_deref() {
        let expires: chrono::DateTime<Utc> = exp
            .parse()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("bad expires_at in DB")))?;
        if expires < Utc::now() {
            return Err(AppError::BadRequest("invite code has expired".into()));
        }
    }
    Ok(Some(Invite {
        code: row.code,
        email: row.email,
        role: row.role,
    }))
}

/// Mark the invite as consumed by `user_id`. Idempotent only in the sense that
/// it will return Err(Conflict) if someone else has already used it.
pub async fn mark_used(
    tx: &mut Transaction<'_, Sqlite>,
    code: &str,
    user_id: &str,
) -> AppResult<()> {
    let res = sqlx::query!(
        "UPDATE invite_codes
         SET used_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), used_by_user_id = ?
         WHERE code = ? AND used_at IS NULL",
        user_id,
        code
    )
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "invite code has already been used".into(),
        ));
    }
    Ok(())
}

/// Idempotently insert the bootstrap admin invite. No-op if a code with the
/// same value already exists (used or not). Called at startup when
/// `INITIAL_INVITE_CODE` is set.
pub async fn ensure_initial_admin(pool: &SqlitePool, code: &str) -> AppResult<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO invite_codes (code, role) VALUES (?, 'admin')",
        code
    )
    .execute(pool)
    .await?;
    Ok(())
}
