//! OAuth (Authorization Code + PKCE in the system browser) for Spotify and
//! Google, the loopback callback listener, and keyring-only token storage.

pub mod google;
pub mod keyring;
pub mod loopback;
pub mod session;
pub mod spotify;

use std::future::Future;
use std::pin::Pin;

use oauth2::basic::BasicTokenResponse;
use oauth2::{RequestTokenError, TokenResponse};
use serde::{Deserialize, Serialize};

use super::error::CoreError;

/// An OAuth token as kept in the OS keyring. `Debug` never prints secrets.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: i64,
    /// Space-separated scopes the token was granted. `None` for tokens saved
    /// before this was recorded.
    #[serde(default)]
    pub scope: Option<String>,
}

impl std::fmt::Debug for StoredToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredToken")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl StoredToken {
    /// Refresh a minute early so a request never races the expiry.
    pub fn needs_refresh(&self, now_unix: i64) -> bool {
        now_unix >= self.expires_at - 60
    }

    /// Build from a token response. A refresh response may omit the refresh
    /// token; in that case the previous one is kept.
    pub fn from_response(
        resp: &BasicTokenResponse,
        previous_refresh: Option<String>,
        now_unix: i64,
    ) -> Self {
        let expires_in = resp
            .expires_in()
            .map(|d| d.as_secs() as i64)
            .unwrap_or(3600);
        StoredToken {
            access_token: resp.access_token().secret().clone(),
            refresh_token: resp
                .refresh_token()
                .map(|r| r.secret().clone())
                .or(previous_refresh),
            expires_at: now_unix + expires_in,
            scope: resp
                .scopes()
                .map(|s| s.iter().map(|x| x.as_str()).collect::<Vec<_>>().join(" ")),
        }
    }

    /// Whether every one of `required` was granted.
    pub fn has_scopes(&self, required: &[&str]) -> bool {
        let granted: Vec<&str> = self
            .scope
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .collect();
        required.iter().all(|r| granted.contains(r))
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Error type for the oauth2 HTTP adapter.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OAuthHttpError(String);

type OAuthFuture =
    Pin<Box<dyn Future<Output = Result<oauth2::HttpResponse, OAuthHttpError>> + Send>>;

/// An HTTP client for oauth2's token requests, built on our reqwest client.
/// Redirects are disabled on this client (oauth2's SSRF recommendation).
pub fn oauth_http(client: reqwest::Client) -> impl Fn(oauth2::HttpRequest) -> OAuthFuture {
    move |req: oauth2::HttpRequest| {
        let client = client.clone();
        Box::pin(async move {
            let req = reqwest::Request::try_from(req)
                .map_err(|e| OAuthHttpError(e.without_url().to_string()))?;
            let resp = client
                .execute(req)
                .await
                .map_err(|e| OAuthHttpError(e.without_url().to_string()))?;
            let mut builder = oauth2::http::Response::builder().status(resp.status());
            for (k, v) in resp.headers() {
                builder = builder.header(k, v);
            }
            let body = resp
                .bytes()
                .await
                .map_err(|e| OAuthHttpError(e.without_url().to_string()))?;
            builder
                .body(body.to_vec())
                .map_err(|e| OAuthHttpError(e.to_string()))
        }) as OAuthFuture
    }
}

/// A reqwest client for token endpoints: no redirects, sane timeouts.
pub fn token_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("playlist-porter/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

/// Map an oauth2 token error to a user-facing message without leaking any
/// response body (it could contain tokens).
pub fn map_token_error<RE: std::error::Error + 'static>(
    e: RequestTokenError<RE, oauth2::basic::BasicErrorResponse>,
) -> CoreError {
    match e {
        RequestTokenError::ServerResponse(r) => {
            let mut msg = r.error().to_string();
            if let Some(d) = r.error_description() {
                msg.push_str(": ");
                msg.push_str(d);
            }
            CoreError::Auth(msg)
        }
        RequestTokenError::Request(e) => CoreError::Network(e.to_string()),
        RequestTokenError::Parse(_, _) => {
            CoreError::Auth("the token endpoint sent an unexpected response".into())
        }
        RequestTokenError::Other(s) => CoreError::Auth(s),
    }
}

/// What the UI needs to show while waiting for the browser.
#[derive(Debug, Clone)]
pub struct AuthStart {
    pub auth_url: String,
    pub port: u16,
}
