mod common;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use backend::auth::invites;
use backend::routes;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn json_request(
    app: axum::Router,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value, Option<String>) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value, cookie)
}

#[tokio::test]
async fn password_signup_consumes_invite_and_issues_tokens() {
    let (state, _tmp) = common::test_state().await;
    invites::ensure_initial_admin(&state.db, "ADMIN-BOOT")
        .await
        .unwrap();

    let app = routes::build(state.clone());

    // Unknown invite → rejected.
    let (status, body, _) = json_request(
        app.clone(),
        "POST",
        "/api/auth/signup/password",
        json!({"code":"nope","email":"a@b.com","password":"longenough1"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");

    // Valid invite → 200, returns access token + admin role.
    let (status, body, set_cookie) = json_request(
        app.clone(),
        "POST",
        "/api/auth/signup/password",
        json!({
            "code":"ADMIN-BOOT",
            "email":"founder@example.com",
            "password":"correct-horse",
            "display_name":"Founder"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
    assert_eq!(body.get("role").and_then(|v| v.as_str()), Some("admin"));
    assert!(
        set_cookie
            .as_deref()
            .unwrap_or("")
            .contains("refresh_token="),
        "expected Set-Cookie with refresh_token, got {set_cookie:?}"
    );

    // The invite is now consumed; a second attempt must fail.
    let (status, body, _) = json_request(
        app,
        "POST",
        "/api/auth/signup/password",
        json!({
            "code":"ADMIN-BOOT",
            "email":"someone@example.com",
            "password":"correct-horse"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");
}

#[tokio::test]
async fn password_login_round_trip() {
    let (state, _tmp) = common::test_state().await;
    invites::ensure_initial_admin(&state.db, "BOOT")
        .await
        .unwrap();
    let app = routes::build(state.clone());

    // sign up
    let (status, body, _) = json_request(
        app.clone(),
        "POST",
        "/api/auth/signup/password",
        json!({"code":"BOOT","email":"u@example.com","password":"longenough1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");

    // Wrong password → 401, no leak of which fact is wrong.
    let (status, _, _) = json_request(
        app.clone(),
        "POST",
        "/api/auth/login",
        json!({"email":"u@example.com","password":"wrong"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Unknown email also 401.
    let (status, _, _) = json_request(
        app.clone(),
        "POST",
        "/api/auth/login",
        json!({"email":"unknown@example.com","password":"longenough1"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Right credentials → 200 + access token.
    let (status, body, _) = json_request(
        app,
        "POST",
        "/api/auth/login",
        json!({"email":"u@example.com","password":"longenough1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
    assert_eq!(body.get("role").and_then(|v| v.as_str()), Some("admin"));
}

#[tokio::test]
async fn invite_bound_email_must_match() {
    let (state, _tmp) = common::test_state().await;
    sqlx::query!(
        "INSERT INTO invite_codes (code, email, role) VALUES (?, ?, 'user')",
        "BOUND",
        "promised@example.com",
    )
    .execute(&state.db)
    .await
    .unwrap();
    let app = routes::build(state.clone());

    // Wrong email → rejected.
    let (status, body, _) = json_request(
        app.clone(),
        "POST",
        "/api/auth/signup/password",
        json!({"code":"BOUND","email":"other@example.com","password":"longenough1"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body: {body}");

    // Right email → success.
    let (status, _, _) = json_request(
        app,
        "POST",
        "/api/auth/signup/password",
        json!({"code":"BOUND","email":"promised@example.com","password":"longenough1"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn check_invite_endpoint_reports_validity_and_bound_email() {
    let (state, _tmp) = common::test_state().await;
    sqlx::query!(
        "INSERT INTO invite_codes (code, email, role) VALUES (?, ?, 'user')",
        "OPENCODE",
        "open@example.com",
    )
    .execute(&state.db)
    .await
    .unwrap();
    let app = routes::build(state);

    let (status, body, _) = json_request(
        app.clone(),
        "POST",
        "/api/auth/signup/invite/check",
        json!({"code":"OPENCODE"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("valid"), Some(&Value::Bool(true)));
    assert_eq!(
        body.get("bound_email").and_then(|v| v.as_str()),
        Some("open@example.com")
    );

    let (status, body, _) = json_request(
        app,
        "POST",
        "/api/auth/signup/invite/check",
        json!({"code":"DOES-NOT-EXIST"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("valid"), Some(&Value::Bool(false)));
}
