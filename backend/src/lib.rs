pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod notes;
pub mod routes;

pub use config::AppConfig;
pub use error::AppError;

use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<AppConfig>,
    pub oauth: Arc<auth::oauth::OAuthClients>,
    pub jwt: Arc<auth::jwt::JwtKeys>,
}
