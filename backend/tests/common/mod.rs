use backend::auth::jwt::JwtKeys;
use backend::auth::oauth::OAuthClients;
use backend::config::AppConfig;
use backend::{AppState, db};
use std::sync::Arc;

pub const TEST_PRIVATE_KEY: &str = include_str!("test_key_private.pem");
pub const TEST_PUBLIC_KEY: &str = include_str!("test_key_public.pem");

pub async fn test_state() -> (AppState, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let url = format!("sqlite://{}?mode=rwc", db_path.display());

    let pool = db::connect(&url).await.expect("connect");
    db::migrate(&pool).await.expect("migrate");

    let config = AppConfig {
        database_url: url,
        bind_addr: "127.0.0.1:0".into(),
        public_base_url: "http://localhost:3000".into(),
        jwt_private_key_pem: TEST_PRIVATE_KEY.into(),
        jwt_public_key_pem: TEST_PUBLIC_KEY.into(),
        jwt_issuer: "test".into(),
        access_token_ttl_secs: 900,
        refresh_token_ttl_secs: 86_400,
        cookie_secret: "test-cookie-secret-very-long-and-fake-fake-fake".into(),
        google: None,
        github: None,
        microsoft: None,
        apple: None,
    };

    let jwt = JwtKeys::new(
        &config.jwt_private_key_pem,
        &config.jwt_public_key_pem,
        config.jwt_issuer.clone(),
        config.access_token_ttl_secs,
    )
    .expect("jwt keys");
    let oauth = OAuthClients::from_config(&config).expect("oauth");

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        oauth: Arc::new(oauth),
        jwt: Arc::new(jwt),
    };
    (state, tmp)
}

pub async fn seed_user(state: &AppState, email: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query!(
        "INSERT INTO users (id, email, display_name) VALUES (?, ?, ?)",
        id,
        email,
        "Test User",
    )
    .execute(&state.db)
    .await
    .expect("seed user");
    id
}
