use crate::AppState;
use crate::error::AppError;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

#[derive(Debug, Clone)]
pub struct AuthUser(pub String);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;
        let claims = state.jwt.verify_access(token)?;
        Ok(AuthUser(claims.sub))
    }
}
