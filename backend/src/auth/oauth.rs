//! OAuth client configuration and provider-specific user-info normalization.
//!
//! Each provider's credentials are optional; if a provider's env vars are not
//! set, its routes will return 503. This lets a fork of the template enable
//! only the providers it needs without code changes.
//!
//! ID-token signature verification is intentionally NOT performed here: the
//! tokens are received directly from the provider over a verified TLS channel
//! during the authorization-code exchange. Projects that need stronger
//! defence-in-depth should fetch the provider's JWKs and verify signatures.

use crate::config::{AppConfig, AppleCreds, MicrosoftCreds, OAuthCreds};
use crate::error::{AppError, AppResult};
use anyhow::{Context, Result, anyhow};
use base64::Engine as _;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
    reqwest::ClientBuilder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Google,
    Github,
    Apple,
    Microsoft,
}

impl FromStr for Provider {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "google" => Ok(Self::Google),
            "github" => Ok(Self::Github),
            "apple" => Ok(Self::Apple),
            "microsoft" => Ok(Self::Microsoft),
            _ => Err(AppError::NotFound),
        }
    }
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Github => "github",
            Self::Apple => "apple",
            Self::Microsoft => "microsoft",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalUser {
    pub provider: Provider,
    pub provider_user_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

pub struct AuthorizeStart {
    pub authorize_url: String,
    pub csrf_state: String,
    pub pkce_verifier: String,
}

type Oauth2Client = oauth2::Client<
    oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
    oauth2::StandardTokenResponse<oauth2::EmptyExtraTokenFields, oauth2::basic::BasicTokenType>,
    oauth2::StandardTokenIntrospectionResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
    oauth2::StandardRevocableToken,
    oauth2::StandardErrorResponse<oauth2::RevocationErrorResponseType>,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
>;

pub struct ProviderClient {
    pub provider: Provider,
    pub client: Oauth2Client,
    pub scopes: Vec<Scope>,
    pub userinfo: UserInfoSource,
    pub public_base_url: String,
    /// For Apple: the client_secret is a per-request signed JWT.
    pub apple: Option<AppleCreds>,
}

#[derive(Clone)]
pub enum UserInfoSource {
    /// Decode the `id_token` (no signature verification — provider-trusted via TLS).
    IdToken,
    /// GET this URL with the access token in `Authorization: Bearer …`.
    GithubUser,
    /// Microsoft Graph `/me` endpoint.
    MicrosoftGraph,
}

pub struct OAuthClients {
    pub http: reqwest::Client,
    pub google: Option<ProviderClient>,
    pub github: Option<ProviderClient>,
    pub apple: Option<ProviderClient>,
    pub microsoft: Option<ProviderClient>,
}

impl OAuthClients {
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        let http = ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("build reqwest client")?;

        Ok(Self {
            http,
            google: config
                .google
                .as_ref()
                .map(|creds| build_google(creds, &config.public_base_url))
                .transpose()?,
            github: config
                .github
                .as_ref()
                .map(|creds| build_github(creds, &config.public_base_url))
                .transpose()?,
            apple: config
                .apple
                .as_ref()
                .map(|creds| build_apple(creds, &config.public_base_url))
                .transpose()?,
            microsoft: config
                .microsoft
                .as_ref()
                .map(|creds| build_microsoft(creds, &config.public_base_url))
                .transpose()?,
        })
    }

    pub fn get(&self, provider: Provider) -> AppResult<&ProviderClient> {
        let opt = match provider {
            Provider::Google => &self.google,
            Provider::Github => &self.github,
            Provider::Apple => &self.apple,
            Provider::Microsoft => &self.microsoft,
        };
        opt.as_ref()
            .ok_or(AppError::ServiceUnavailable(match provider {
                Provider::Google => "google oauth",
                Provider::Github => "github oauth",
                Provider::Apple => "apple oauth",
                Provider::Microsoft => "microsoft oauth",
            }))
    }

    pub fn start(&self, provider: Provider) -> AppResult<AuthorizeStart> {
        let pc = self.get(provider)?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut req = pc
            .client
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(challenge);
        for s in &pc.scopes {
            req = req.add_scope(s.clone());
        }
        let (url, csrf) = req.url();
        Ok(AuthorizeStart {
            authorize_url: url.to_string(),
            csrf_state: csrf.secret().to_string(),
            pkce_verifier: verifier.secret().to_string(),
        })
    }

    pub async fn complete(
        &self,
        provider: Provider,
        code: String,
        verifier: String,
    ) -> AppResult<ExternalUser> {
        let pc = self.get(provider)?;
        let pkce = PkceCodeVerifier::new(verifier);

        // Apple requires a fresh signed-JWT client_secret per request, so we
        // rebuild the client each time with the minted secret. Other providers
        // use the cached client with a stored static secret.
        let token = if let (Provider::Apple, Some(apple)) = (provider, pc.apple.as_ref()) {
            let secret = mint_apple_client_secret(apple)
                .map_err(|e| AppError::Internal(anyhow!("apple client_secret: {e}")))?;
            let client = build_apple_with_secret(apple, &pc.public_base_url, &secret)
                .map_err(|e| AppError::Internal(anyhow!("build apple client: {e}")))?;
            client
                .exchange_code(AuthorizationCode::new(code))
                .set_pkce_verifier(pkce)
                .request_async(&self.http)
                .await
                .map_err(|e| AppError::Internal(anyhow!("oauth exchange: {e}")))?
        } else {
            pc.client
                .exchange_code(AuthorizationCode::new(code))
                .set_pkce_verifier(pkce)
                .request_async(&self.http)
                .await
                .map_err(|e| AppError::Internal(anyhow!("oauth exchange: {e}")))?
        };

        match pc.userinfo {
            UserInfoSource::IdToken => extract_from_id_token(provider, &token),
            UserInfoSource::GithubUser => fetch_github_user(&self.http, &token).await,
            UserInfoSource::MicrosoftGraph => fetch_microsoft_user(&self.http, &token).await,
        }
    }
}

fn redirect_for(public_base_url: &str, provider: Provider) -> Result<RedirectUrl> {
    let url = format!(
        "{}/api/auth/{}/callback",
        public_base_url.trim_end_matches('/'),
        provider.as_str()
    );
    Ok(RedirectUrl::new(url)?)
}

fn build_google(creds: &OAuthCreds, public_base_url: &str) -> Result<ProviderClient> {
    let client = BasicClient::new(ClientId::new(creds.client_id.clone()))
        .set_client_secret(ClientSecret::new(creds.client_secret.clone()))
        .set_auth_uri(AuthUrl::new(
            "https://accounts.google.com/o/oauth2/v2/auth".into(),
        )?)
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".into())?)
        .set_redirect_uri(redirect_for(public_base_url, Provider::Google)?);
    Ok(ProviderClient {
        provider: Provider::Google,
        client,
        scopes: vec![
            Scope::new("openid".into()),
            Scope::new("email".into()),
            Scope::new("profile".into()),
        ],
        userinfo: UserInfoSource::IdToken,
        public_base_url: public_base_url.to_string(),
        apple: None,
    })
}

fn build_github(creds: &OAuthCreds, public_base_url: &str) -> Result<ProviderClient> {
    let client = BasicClient::new(ClientId::new(creds.client_id.clone()))
        .set_client_secret(ClientSecret::new(creds.client_secret.clone()))
        .set_auth_uri(AuthUrl::new(
            "https://github.com/login/oauth/authorize".into(),
        )?)
        .set_token_uri(TokenUrl::new(
            "https://github.com/login/oauth/access_token".into(),
        )?)
        .set_redirect_uri(redirect_for(public_base_url, Provider::Github)?);
    Ok(ProviderClient {
        provider: Provider::Github,
        client,
        scopes: vec![
            Scope::new("read:user".into()),
            Scope::new("user:email".into()),
        ],
        userinfo: UserInfoSource::GithubUser,
        public_base_url: public_base_url.to_string(),
        apple: None,
    })
}

fn build_apple(creds: &AppleCreds, public_base_url: &str) -> Result<ProviderClient> {
    // Placeholder client (no usable secret) — the real client is rebuilt at
    // exchange time with a freshly-minted JWT secret. The placeholder is only
    // used to construct the authorize URL in `start()`.
    let client = BasicClient::new(ClientId::new(creds.client_id.clone()))
        .set_auth_uri(AuthUrl::new(
            "https://appleid.apple.com/auth/authorize".into(),
        )?)
        .set_token_uri(TokenUrl::new(
            "https://appleid.apple.com/auth/token".into(),
        )?)
        .set_redirect_uri(redirect_for(public_base_url, Provider::Apple)?);
    Ok(ProviderClient {
        provider: Provider::Apple,
        client,
        scopes: vec![Scope::new("name".into()), Scope::new("email".into())],
        userinfo: UserInfoSource::IdToken,
        public_base_url: public_base_url.to_string(),
        apple: Some(creds.clone()),
    })
}

fn build_apple_with_secret(
    creds: &AppleCreds,
    public_base_url: &str,
    secret: &str,
) -> Result<Oauth2Client> {
    let client = BasicClient::new(ClientId::new(creds.client_id.clone()))
        .set_client_secret(ClientSecret::new(secret.to_string()))
        .set_auth_uri(AuthUrl::new(
            "https://appleid.apple.com/auth/authorize".into(),
        )?)
        .set_token_uri(TokenUrl::new(
            "https://appleid.apple.com/auth/token".into(),
        )?)
        .set_redirect_uri(redirect_for(public_base_url, Provider::Apple)?);
    Ok(client)
}

fn build_microsoft(creds: &MicrosoftCreds, public_base_url: &str) -> Result<ProviderClient> {
    let auth_url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
        creds.tenant
    );
    let token_url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        creds.tenant
    );
    let client = BasicClient::new(ClientId::new(creds.client_id.clone()))
        .set_client_secret(ClientSecret::new(creds.client_secret.clone()))
        .set_auth_uri(AuthUrl::new(auth_url)?)
        .set_token_uri(TokenUrl::new(token_url)?)
        .set_redirect_uri(redirect_for(public_base_url, Provider::Microsoft)?);
    Ok(ProviderClient {
        provider: Provider::Microsoft,
        client,
        scopes: vec![
            Scope::new("openid".into()),
            Scope::new("email".into()),
            Scope::new("profile".into()),
            Scope::new("User.Read".into()),
        ],
        userinfo: UserInfoSource::MicrosoftGraph,
        public_base_url: public_base_url.to_string(),
        apple: None,
    })
}

fn extract_from_id_token(
    provider: Provider,
    token: &oauth2::StandardTokenResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
) -> AppResult<ExternalUser> {
    // The basic StandardTokenResponse does not expose `id_token` as an
    // extra field; we serialize the whole response to JSON and re-read it.
    let json = serde_json::to_value(token)
        .map_err(|e| AppError::Internal(anyhow!("serialize token: {e}")))?;
    let id_token = json
        .get("id_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Internal(anyhow!("missing id_token from {}", provider.as_str()))
        })?;
    let claims = decode_jwt_claims(id_token)?;

    let sub = claims
        .get("sub")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal(anyhow!("id_token missing sub")))?
        .to_string();
    let email = claims
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("provider did not return an email".into()))?
        .to_string();
    let display_name = claims
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let avatar_url = claims
        .get("picture")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ExternalUser {
        provider,
        provider_user_id: sub,
        email,
        display_name,
        avatar_url,
    })
}

fn decode_jwt_claims(token: &str) -> AppResult<Value> {
    let mut parts = token.split('.');
    let _header = parts.next();
    let payload = parts
        .next()
        .ok_or_else(|| AppError::Internal(anyhow!("malformed jwt")))?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|e| AppError::Internal(anyhow!("jwt base64: {e}")))?;
    serde_json::from_slice::<Value>(&decoded)
        .map_err(|e| AppError::Internal(anyhow!("jwt json: {e}")))
}

async fn fetch_github_user(
    http: &reqwest::Client,
    token: &oauth2::StandardTokenResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
) -> AppResult<ExternalUser> {
    let access = token.access_token().secret();
    let me: Value = http
        .get("https://api.github.com/user")
        .bearer_auth(access)
        .header("User-Agent", "full-stack-template")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow!("github /user: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(anyhow!("github /user status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow!("github /user json: {e}")))?;

    let id = me
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| AppError::Internal(anyhow!("github /user missing id")))?
        .to_string();
    let display_name = me
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| me.get("login").and_then(|v| v.as_str()))
        .map(|s| s.to_string());
    let avatar_url = me
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let email = match me.get("email").and_then(|v| v.as_str()) {
        Some(e) => e.to_string(),
        None => fetch_github_primary_email(http, access).await?,
    };

    Ok(ExternalUser {
        provider: Provider::Github,
        provider_user_id: id,
        email,
        display_name,
        avatar_url,
    })
}

async fn fetch_github_primary_email(http: &reqwest::Client, access: &str) -> AppResult<String> {
    #[derive(Deserialize)]
    struct Email {
        email: String,
        primary: bool,
        verified: bool,
    }
    let emails: Vec<Email> = http
        .get("https://api.github.com/user/emails")
        .bearer_auth(access)
        .header("User-Agent", "full-stack-template")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow!("github /user/emails: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(anyhow!("github /user/emails status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow!("github /user/emails json: {e}")))?;
    emails
        .into_iter()
        .find(|e| e.primary && e.verified)
        .map(|e| e.email)
        .ok_or_else(|| AppError::BadRequest("no verified primary email on github account".into()))
}

async fn fetch_microsoft_user(
    http: &reqwest::Client,
    token: &oauth2::StandardTokenResponse<
        oauth2::EmptyExtraTokenFields,
        oauth2::basic::BasicTokenType,
    >,
) -> AppResult<ExternalUser> {
    let access = token.access_token().secret();
    let me: Value = http
        .get("https://graph.microsoft.com/v1.0/me")
        .bearer_auth(access)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow!("graph /me: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Internal(anyhow!("graph /me status: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow!("graph /me json: {e}")))?;

    let id = me
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal(anyhow!("graph /me missing id")))?
        .to_string();
    let email = me
        .get("mail")
        .and_then(|v| v.as_str())
        .or_else(|| me.get("userPrincipalName").and_then(|v| v.as_str()))
        .ok_or_else(|| AppError::BadRequest("microsoft account has no email".into()))?
        .to_string();
    let display_name = me
        .get("displayName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(ExternalUser {
        provider: Provider::Microsoft,
        provider_user_id: id,
        email,
        display_name,
        avatar_url: None,
    })
}

fn mint_apple_client_secret(creds: &AppleCreds) -> Result<String> {
    #[derive(Serialize)]
    struct AppleClaims {
        iss: String,
        iat: i64,
        exp: i64,
        aud: &'static str,
        sub: String,
    }
    let now = chrono::Utc::now().timestamp();
    let claims = AppleClaims {
        iss: creds.team_id.clone(),
        iat: now,
        exp: now + 60 * 60,
        aud: "https://appleid.apple.com",
        sub: creds.client_id.clone(),
    };
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(creds.key_id.clone());
    let key = EncodingKey::from_ec_pem(creds.private_key_pem.as_bytes())
        .context("parse apple private key")?;
    let token = jsonwebtoken::encode(&header, &claims, &key).context("sign apple client_secret")?;
    Ok(token)
}
