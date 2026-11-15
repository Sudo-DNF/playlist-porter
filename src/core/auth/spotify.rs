//! Spotify Authorization Code + PKCE. No client secret.

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, RefreshToken, Scope, TokenUrl,
};

use super::{map_token_error, now_unix, oauth_http, StoredToken};
use crate::core::error::{CoreError, CoreResult};

pub const AUTH_URL: &str = "https://accounts.spotify.com/authorize";
pub const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
/// Exactly these scopes; do not add any. `user-library-read` is only for
/// reading Liked Songs (`GET /me/tracks`).
pub const SCOPES: [&str; 3] = [
    "playlist-read-private",
    "playlist-read-collaborative",
    "user-library-read",
];

/// Secrets held only in memory for the duration of one sign-in.
pub struct PendingAuth {
    pub auth_url: String,
    pub csrf_state: String,
    pub verifier: PkceCodeVerifier,
}

fn urls(redirect_uri: &str) -> CoreResult<(AuthUrl, TokenUrl, RedirectUrl)> {
    let bad = |e: url::ParseError| CoreError::Config(format!("invalid URL: {e}"));
    Ok((
        AuthUrl::new(AUTH_URL.into()).map_err(bad)?,
        TokenUrl::new(TOKEN_URL.into()).map_err(bad)?,
        RedirectUrl::new(redirect_uri.into()).map_err(bad)?,
    ))
}

/// Build the authorize URL with a fresh PKCE verifier and CSRF state.
pub fn begin(client_id: &str, redirect_uri: &str) -> CoreResult<PendingAuth> {
    let (auth, token, redirect) = urls(redirect_uri)?;
    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(auth)
        .set_token_uri(token)
        .set_redirect_uri(redirect);
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut req = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(challenge);
    for s in SCOPES {
        req = req.add_scope(Scope::new(s.to_string()));
    }
    let (url, csrf) = req.url();
    Ok(PendingAuth {
        auth_url: url.to_string(),
        csrf_state: csrf.secret().clone(),
        verifier,
    })
}

pub async fn exchange(
    http: &reqwest::Client,
    client_id: &str,
    redirect_uri: &str,
    code: String,
    verifier: PkceCodeVerifier,
) -> CoreResult<StoredToken> {
    let (auth, token, redirect) = urls(redirect_uri)?;
    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_auth_uri(auth)
        .set_token_uri(token)
        .set_redirect_uri(redirect);
    let http = oauth_http(http.clone());
    let resp = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(&http)
        .await
        .map_err(map_token_error)?;
    Ok(StoredToken::from_response(&resp, None, now_unix()))
}

/// Refresh; Spotify may rotate the refresh token, otherwise the old one is kept.
pub async fn refresh(
    http: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> CoreResult<StoredToken> {
    let token = TokenUrl::new(TOKEN_URL.into())
        .map_err(|e| CoreError::Config(format!("invalid URL: {e}")))?;
    let client = BasicClient::new(ClientId::new(client_id.to_string())).set_token_uri(token);
    let http = oauth_http(http.clone());
    let rt = RefreshToken::new(refresh_token.to_string());
    let resp = client
        .exchange_refresh_token(&rt)
        .request_async(&http)
        .await
        .map_err(map_token_error)?;
    Ok(StoredToken::from_response(
        &resp,
        Some(refresh_token.to_string()),
        now_unix(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorize_url_has_pkce_exact_scopes_and_loopback_ip() {
        let p = begin("cid", "http://127.0.0.1:8888/callback").unwrap();
        let url = url::Url::parse(&p.auth_url).unwrap();
        assert_eq!(url.host_str(), Some("accounts.spotify.com"));
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:8888/callback");
        assert_eq!(
            q["scope"],
            "playlist-read-private playlist-read-collaborative user-library-read"
        );
        assert_eq!(q["code_challenge_method"], "S256");
        assert!(q["code_challenge"].len() >= 43);
        assert_eq!(q["state"], p.csrf_state);
        assert!(!q.contains_key("client_secret"));
    }
}
