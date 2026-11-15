//! Google OAuth for the YouTube Data API: "Desktop app" client, Authorization
//! Code + PKCE in the system browser, loopback redirect. Google desktop
//! clients also need the client secret (from config/env, never hard-coded).

use oauth2::basic::BasicClient;
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenUrl,
};

use super::spotify::PendingAuth;
use super::{map_token_error, now_unix, oauth_http, StoredToken};
use crate::core::error::{CoreError, CoreResult};

pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
/// The only scope requested: needed to create playlists and insert items.
pub const SCOPE: &str = "https://www.googleapis.com/auth/youtube";

pub struct GoogleApp<'a> {
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub redirect_uri: &'a str,
}

fn invalid(e: url::ParseError) -> CoreError {
    CoreError::Config(format!("invalid URL: {e}"))
}

pub fn begin(app: &GoogleApp<'_>) -> CoreResult<PendingAuth> {
    let client = BasicClient::new(ClientId::new(app.client_id.into()))
        .set_client_secret(ClientSecret::new(app.client_secret.into()))
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::new(AUTH_URL.into()).map_err(invalid)?)
        .set_token_uri(TokenUrl::new(TOKEN_URL.into()).map_err(invalid)?)
        .set_redirect_uri(RedirectUrl::new(app.redirect_uri.into()).map_err(invalid)?);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(SCOPE.into()))
        .set_pkce_challenge(challenge)
        // A refresh token, so long transfers survive the 1-hour access token.
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .url();
    Ok(PendingAuth {
        auth_url: url.to_string(),
        csrf_state: csrf.secret().clone(),
        verifier,
    })
}

pub async fn exchange(
    http: &reqwest::Client,
    app: &GoogleApp<'_>,
    code: String,
    verifier: PkceCodeVerifier,
) -> CoreResult<StoredToken> {
    let client = BasicClient::new(ClientId::new(app.client_id.into()))
        .set_client_secret(ClientSecret::new(app.client_secret.into()))
        .set_auth_type(AuthType::RequestBody)
        .set_token_uri(TokenUrl::new(TOKEN_URL.into()).map_err(invalid)?)
        .set_redirect_uri(RedirectUrl::new(app.redirect_uri.into()).map_err(invalid)?);
    let http = oauth_http(http.clone());
    let resp = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(&http)
        .await
        .map_err(map_token_error)?;
    Ok(StoredToken::from_response(&resp, None, now_unix()))
}

pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> CoreResult<StoredToken> {
    let client = BasicClient::new(ClientId::new(client_id.into()))
        .set_client_secret(ClientSecret::new(client_secret.into()))
        .set_auth_type(AuthType::RequestBody)
        .set_token_uri(TokenUrl::new(TOKEN_URL.into()).map_err(invalid)?);
    let http = oauth_http(http.clone());
    let rt = RefreshToken::new(refresh_token.into());
    let resp = client
        .exchange_refresh_token(&rt)
        .request_async(&http)
        .await
        .map_err(map_token_error)?;
    Ok(StoredToken::from_response(
        &resp,
        Some(refresh_token.into()),
        now_unix(),
    ))
}

/// Revoke the grant (YouTube policy III.D.2.c). Revoking the refresh token
/// also invalidates its access tokens. "Already invalid" counts as success.
pub async fn revoke(http: &reqwest::Client, token: &StoredToken) -> CoreResult<()> {
    let t = token
        .refresh_token
        .as_deref()
        .unwrap_or(&token.access_token);
    let resp = http
        .post(REVOKE_URL)
        .form(&[("token", t)])
        .send()
        .await
        .map_err(CoreError::from)?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 400 {
        // 400 invalid_token: already revoked or expired.
        Ok(())
    } else {
        Err(CoreError::Http {
            service: "Google",
            status: status.as_u16(),
            message: "token revocation failed".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_requests_only_youtube_scope() {
        let app = GoogleApp {
            client_id: "id.apps.googleusercontent.com",
            client_secret: "s3cret",
            redirect_uri: "http://127.0.0.1:8888/callback",
        };
        let p = begin(&app).unwrap();
        let url = url::Url::parse(&p.auth_url).unwrap();
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["scope"], SCOPE);
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:8888/callback");
        // The secret is sent only to the token endpoint, never in the browser URL.
        assert!(!p.auth_url.contains("s3cret"));
    }
}
