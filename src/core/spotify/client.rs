//! Minimal Spotify Web API client: exactly four endpoints, all `GET`.
//!
//! - `GET /v1/me`
//! - `GET /v1/me/playlists?limit=50` (paginated with `next`)
//! - `GET /v1/playlists/{id}`
//! - `GET /v1/playlists/{id}/items?limit=50` (paginated with `next`)

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;

use super::models::{api_error_message, Entry, Paging, Playlist, PlaylistEntry, User};
use crate::core::auth::session::Session;
use crate::core::error::{CoreError, CoreResult};

pub const API_BASE: &str = "https://api.spotify.com/v1";

/// Retry caps for 429 and 5xx.
const MAX_RATE_RETRIES: u32 = 6;
const MAX_RETRY_WAIT_SECS: u64 = 300;
const MAX_SERVER_RETRIES: u32 = 3;
/// Guard against a `next` loop.
const MAX_PAGES: usize = 2000;

/// Progress notices for the UI (e.g. "Rate limited, retrying in N s").
pub type Notify = Arc<dyn Fn(u64) + Send + Sync>;

/// Who may be processed (Policy III.9): the owner, or anyone on a
/// collaborative playlist.
pub fn is_exportable(owner_id: &str, collaborative: bool, me_id: &str) -> bool {
    collaborative || owner_id == me_id
}

pub fn ensure_exportable(p: &Playlist, me_id: &str) -> CoreResult<()> {
    if is_exportable(&p.owner.id, p.collaborative, me_id) {
        Ok(())
    } else {
        Err(CoreError::NotExportable)
    }
}

/// Spotify IDs are base62. Validated before use in URLs and file names.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// What to do with a 429.
#[derive(Debug, PartialEq, Eq)]
pub enum RateLimit {
    /// Daily/app quota exhausted: stop with a clear message, don't loop.
    QuotaExceeded,
    RetryAfter(u64),
}

/// Parse a 429 response: `Retry-After` is in seconds; default 1.
pub fn parse_429(retry_after: Option<&str>, body: &str) -> RateLimit {
    if body.contains("QUOTA_EXCEEDED") {
        return RateLimit::QuotaExceeded;
    }
    let secs = retry_after
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(1)
        .max(1);
    RateLimit::RetryAfter(secs)
}

/// Seconds to wait before retry `attempt` (0-based): at least Retry-After,
/// growing exponentially.
pub fn backoff_secs(retry_after: u64, attempt: u32) -> u64 {
    retry_after.max(1u64 << attempt.min(6))
}

/// `next` URLs must stay on the API host so the bearer token never goes
/// anywhere else.
pub fn is_api_url(u: &str) -> bool {
    u.starts_with(&format!("{API_BASE}/"))
}

/// Follow `next` links until exhausted.
pub async fn collect_pages<T, F, Fut>(first: String, mut fetch: F) -> CoreResult<Vec<T>>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = CoreResult<Paging<T>>>,
{
    let mut out = Vec::new();
    let mut next = Some(first);
    let mut pages = 0;
    while let Some(url) = next.take() {
        if !is_api_url(&url) {
            return Err(CoreError::Network(
                "Spotify returned a page link outside api.spotify.com; stopped".into(),
            ));
        }
        pages += 1;
        if pages > MAX_PAGES {
            return Err(CoreError::Network("too many pages from Spotify".into()));
        }
        let page = fetch(url).await?;
        out.extend(page.items);
        next = page.next.filter(|n| !n.is_empty());
    }
    Ok(out)
}

pub struct SpotifyClient {
    http: reqwest::Client,
    session: Arc<Session>,
    on_rate_limit: Notify,
}

impl SpotifyClient {
    pub fn new(http: reqwest::Client, session: Arc<Session>, on_rate_limit: Notify) -> Self {
        SpotifyClient {
            http,
            session,
            on_rate_limit,
        }
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> CoreResult<T> {
        if !is_api_url(url) {
            return Err(CoreError::Network("refusing a non-Spotify API URL".into()));
        }
        let mut token = self.session.access_token().await?;
        let mut refreshed = false;
        let mut rate_attempts = 0;
        let mut server_attempts = 0;
        loop {
            let resp = self.http.get(url).bearer_auth(&token).send().await?;
            let status = resp.status().as_u16();
            match status {
                200..=299 => {
                    let bytes = resp.bytes().await?;
                    return serde_json::from_slice(&bytes).map_err(|e| {
                        CoreError::Network(format!("unexpected response from Spotify: {e}"))
                    });
                }
                401 if !refreshed => {
                    refreshed = true;
                    token = self.session.refresh_after_401(&token).await?;
                }
                401 => return Err(CoreError::SessionExpired),
                403 => {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(CoreError::SpotifyForbidden {
                        spotify_message: api_error_message(&body),
                    });
                }
                429 => {
                    let ra = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned);
                    let body = resp.text().await.unwrap_or_default();
                    match parse_429(ra.as_deref(), &body) {
                        RateLimit::QuotaExceeded => return Err(CoreError::SpotifyQuotaExceeded),
                        RateLimit::RetryAfter(secs) => {
                            if rate_attempts >= MAX_RATE_RETRIES || secs > MAX_RETRY_WAIT_SECS {
                                return Err(CoreError::RateLimitGaveUp);
                            }
                            let wait = backoff_secs(secs, rate_attempts);
                            rate_attempts += 1;
                            (self.on_rate_limit)(wait);
                            tokio::time::sleep(Duration::from_secs(wait)).await;
                            (self.on_rate_limit)(0);
                        }
                    }
                }
                500..=599 if server_attempts < MAX_SERVER_RETRIES => {
                    server_attempts += 1;
                    tokio::time::sleep(Duration::from_secs(1 << server_attempts)).await;
                }
                _ => {
                    let body = resp.text().await.unwrap_or_default();
                    let mut message = api_error_message(&body);
                    if message.is_empty() {
                        message = "request failed".into();
                    }
                    return Err(CoreError::Http {
                        service: "Spotify",
                        status,
                        message,
                    });
                }
            }
        }
    }

    pub async fn me(&self) -> CoreResult<User> {
        self.get_json(&format!("{API_BASE}/me")).await
    }

    pub async fn my_playlists(&self) -> CoreResult<Vec<Playlist>> {
        collect_pages(
            format!("{API_BASE}/me/playlists?limit=50"),
            |u| async move { self.get_json::<Paging<Playlist>>(&u).await },
        )
        .await
    }

    pub async fn playlist(&self, id: &str) -> CoreResult<Playlist> {
        if !is_valid_id(id) {
            return Err(CoreError::NotExportable);
        }
        self.get_json(&format!("{API_BASE}/playlists/{id}")).await
    }

    pub async fn playlist_entries(&self, id: &str) -> CoreResult<Vec<Entry>> {
        if !is_valid_id(id) {
            return Err(CoreError::NotExportable);
        }
        let raw = collect_pages(
            format!("{API_BASE}/playlists/{id}/items?limit=50"),
            |u| async move { self.get_json::<Paging<PlaylistEntry>>(&u).await },
        )
        .await?;
        Ok(raw.iter().map(Entry::from_api).collect())
    }

    /// Fetch a playlist for export/transfer, re-checking ownership so the UI
    /// can't bypass it.
    pub async fn load_for_processing(
        &self,
        id: &str,
        me_id: &str,
    ) -> CoreResult<(Playlist, Vec<Entry>)> {
        let p = self.playlist(id).await?;
        ensure_exportable(&p, me_id)?;
        let entries = self.playlist_entries(id).await?;
        Ok((p, entries))
    }
}
