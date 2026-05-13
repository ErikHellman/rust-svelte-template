use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: String,
    pub public_base_url: String,
    pub jwt_private_key_pem: String,
    pub jwt_public_key_pem: String,
    pub jwt_issuer: String,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
    pub cookie_secret: String,

    pub google: Option<OAuthCreds>,
    pub github: Option<OAuthCreds>,
    pub microsoft: Option<MicrosoftCreds>,
    pub apple: Option<AppleCreds>,
}

#[derive(Debug, Clone)]
pub struct OAuthCreds {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone)]
pub struct MicrosoftCreds {
    pub client_id: String,
    pub client_secret: String,
    pub tenant: String,
}

#[derive(Debug, Clone)]
pub struct AppleCreds {
    pub client_id: String,
    pub team_id: String,
    pub key_id: String,
    pub private_key_pem: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            bind_addr: optional("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:3000".to_string()),
            public_base_url: required("PUBLIC_BASE_URL")?,
            jwt_private_key_pem: required("JWT_PRIVATE_KEY_PEM")?,
            jwt_public_key_pem: required("JWT_PUBLIC_KEY_PEM")?,
            jwt_issuer: optional("JWT_ISSUER").unwrap_or_else(|| "full-stack-template".to_string()),
            access_token_ttl_secs: optional("ACCESS_TOKEN_TTL_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(15 * 60),
            refresh_token_ttl_secs: optional("REFRESH_TOKEN_TTL_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30 * 24 * 60 * 60),
            cookie_secret: required("COOKIE_SECRET")?,

            google: oauth_pair("GOOGLE"),
            github: oauth_pair("GITHUB"),
            microsoft: microsoft_creds(),
            apple: apple_creds(),
        })
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional(key: &str) -> Option<String> {
    env::var(key).ok().filter(|s| !s.is_empty())
}

fn oauth_pair(prefix: &str) -> Option<OAuthCreds> {
    let id = optional(&format!("{prefix}_CLIENT_ID"))?;
    let secret = optional(&format!("{prefix}_CLIENT_SECRET"))?;
    Some(OAuthCreds {
        client_id: id,
        client_secret: secret,
    })
}

fn microsoft_creds() -> Option<MicrosoftCreds> {
    let client_id = optional("MICROSOFT_CLIENT_ID")?;
    let client_secret = optional("MICROSOFT_CLIENT_SECRET")?;
    let tenant = optional("MICROSOFT_TENANT").unwrap_or_else(|| "common".to_string());
    Some(MicrosoftCreds {
        client_id,
        client_secret,
        tenant,
    })
}

fn apple_creds() -> Option<AppleCreds> {
    Some(AppleCreds {
        client_id: optional("APPLE_CLIENT_ID")?,
        team_id: optional("APPLE_TEAM_ID")?,
        key_id: optional("APPLE_KEY_ID")?,
        private_key_pem: optional("APPLE_PRIVATE_KEY_PEM")?,
    })
}
