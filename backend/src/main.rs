use anyhow::{Context, Result};
use backend::auth::{invites, jwt::JwtKeys, oauth::OAuthClients};
use backend::{AppConfig, AppState, db, routes};
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = AppConfig::from_env().context("loading config")?;
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    if let Some(code) = config.initial_invite_code.as_deref() {
        invites::ensure_initial_admin(&pool, code).await?;
        tracing::info!("initial admin invite code ensured");
    }

    let jwt = JwtKeys::new(
        &config.jwt_private_key_pem,
        &config.jwt_public_key_pem,
        config.jwt_issuer.clone(),
        config.access_token_ttl_secs,
    )?;
    let oauth = OAuthClients::from_config(&config)?;

    let state = AppState {
        db: pool,
        config: Arc::new(config),
        oauth: Arc::new(oauth),
        jwt: Arc::new(jwt),
    };

    let app = routes::build(state.clone());
    let listener = tokio::net::TcpListener::bind(&state.config.bind_addr).await?;
    tracing::info!(addr = %state.config.bind_addr, "backend listening");
    axum::serve(listener, app).await?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,backend=debug,tower_http=debug"));
    fmt().with_env_filter(filter).with_target(false).init();
}
