use crate::error::{AppError, AppResult};
use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iss: String,
    pub iat: i64,
    pub exp: i64,
}

pub struct JwtKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
    issuer: String,
    access_ttl_secs: i64,
}

impl JwtKeys {
    pub fn new(
        private_pem: &str,
        public_pem: &str,
        issuer: String,
        access_ttl_secs: i64,
    ) -> Result<Self> {
        let encoding = EncodingKey::from_rsa_pem(private_pem.as_bytes())
            .context("failed to parse JWT private key (expected RSA PEM)")?;
        let decoding = DecodingKey::from_rsa_pem(public_pem.as_bytes())
            .context("failed to parse JWT public key (expected RSA PEM)")?;
        Ok(Self {
            encoding,
            decoding,
            issuer,
            access_ttl_secs,
        })
    }

    pub fn mint_access(&self, user_id: &str) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        let claims = Claims {
            sub: user_id.to_string(),
            iss: self.issuer.clone(),
            iat: now,
            exp: now + self.access_ttl_secs,
        };
        let token = encode(&Header::new(Algorithm::RS256), &claims, &self.encoding)?;
        Ok(token)
    }

    pub fn verify_access(&self, token: &str) -> AppResult<Claims> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        let data = decode::<Claims>(token, &self.decoding, &validation)
            .map_err(|_| AppError::Unauthorized)?;
        Ok(data.claims)
    }
}
