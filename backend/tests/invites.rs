mod common;

use backend::auth::{invites, password};

async fn seed_invite(pool: &sqlx::SqlitePool, code: &str, email: Option<&str>, role: &str) {
    sqlx::query!(
        "INSERT INTO invite_codes (code, email, role) VALUES (?, ?, ?)",
        code,
        email,
        role
    )
    .execute(pool)
    .await
    .expect("seed invite");
}

#[tokio::test]
async fn find_valid_returns_none_for_unknown_code() {
    let (state, _tmp) = common::test_state().await;
    let r = invites::find_valid(&state.db, "nope").await.unwrap();
    assert!(r.is_none());
}

#[tokio::test]
async fn find_valid_returns_invite_then_consumes_once() {
    let (state, _tmp) = common::test_state().await;
    seed_invite(&state.db, "ABCD-1234", Some("vip@example.com"), "admin").await;

    let invite = invites::find_valid(&state.db, "ABCD-1234")
        .await
        .unwrap()
        .expect("present");
    assert_eq!(invite.email.as_deref(), Some("vip@example.com"));
    assert_eq!(invite.role, "admin");

    // Consume it inside a transaction.
    let mut tx = state.db.begin().await.unwrap();
    let fake_user_id = "00000000-0000-0000-0000-000000000000";
    sqlx::query!(
        "INSERT INTO users (id, email, role) VALUES (?, ?, 'admin')",
        fake_user_id,
        "vip@example.com"
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    invites::mark_used(&mut tx, "ABCD-1234", fake_user_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // After consumption, find_valid should error (BadRequest), not return Ok(Some).
    let err = invites::find_valid(&state.db, "ABCD-1234").await;
    assert!(err.is_err(), "consumed invite must not be reusable");

    // A second mark_used must also fail.
    let mut tx = state.db.begin().await.unwrap();
    let second = invites::mark_used(&mut tx, "ABCD-1234", fake_user_id).await;
    assert!(second.is_err(), "double-spend must be rejected");
}

#[tokio::test]
async fn ensure_initial_admin_is_idempotent_and_creates_admin_invite() {
    let (state, _tmp) = common::test_state().await;
    invites::ensure_initial_admin(&state.db, "BOOTSTRAP")
        .await
        .unwrap();
    invites::ensure_initial_admin(&state.db, "BOOTSTRAP")
        .await
        .unwrap();

    let invite = invites::find_valid(&state.db, "BOOTSTRAP")
        .await
        .unwrap()
        .expect("present");
    assert_eq!(invite.role, "admin");
    assert!(invite.email.is_none());
}

#[tokio::test]
async fn password_hash_round_trip() {
    let hash = password::hash("hunter2-correct-horse").unwrap();
    password::verify(&hash, "hunter2-correct-horse").unwrap();
    let bad = password::verify(&hash, "wrong-password");
    assert!(bad.is_err());
}

#[tokio::test]
async fn password_strength_floor() {
    assert!(password::validate_strength("short").is_err());
    assert!(password::validate_strength("longenough").is_ok());
}
