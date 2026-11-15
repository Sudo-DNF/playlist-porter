//! YouTube Data API v3 client. Only `www.googleapis.com/youtube/v3`:
//! `search.list`, `videos.list`, `playlists.insert`, `playlistItems.insert`,
//! and `channels.list?mine=true` for the account name shown in the UI.
//! Nothing is ever played or downloaded.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use super::transfer::{SearchHit, VideoApi};
use crate::core::auth::session::Session;
use crate::core::error::{CoreError, CoreResult};
use crate::core::matching::parse_iso8601_duration;

pub const API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Deserialize)]
struct ErrBody {
    error: ErrInner,
}

#[derive(Debug, Deserialize)]
struct ErrInner {
    #[serde(default)]
    message: String,
    #[serde(default)]
    errors: Vec<ErrDetail>,
}

#[derive(Debug, Deserialize)]
struct ErrDetail {
    #[serde(default)]
    reason: String,
}

/// What to do with an error response.
#[derive(Debug, PartialEq, Eq)]
pub enum ErrorKind {
    QuotaExceeded,
    Retry,
    Fatal(CoreError),
}

/// Classify a YouTube error response (pure, for tests).
pub fn classify_error(status: u16, body: &str) -> ErrorKind {
    let parsed = serde_json::from_str::<ErrBody>(body).ok();
    let reasons: Vec<&str> = parsed
        .as_ref()
        .map(|b| b.error.errors.iter().map(|e| e.reason.as_str()).collect())
        .unwrap_or_default();
    if reasons
        .iter()
        .any(|r| matches!(*r, "quotaExceeded" | "dailyLimitExceeded"))
    {
        return ErrorKind::QuotaExceeded;
    }
    if status >= 500
        || status == 429
        || reasons
            .iter()
            .any(|r| matches!(*r, "rateLimitExceeded" | "userRateLimitExceeded"))
    {
        return ErrorKind::Retry;
    }
    let message = parsed
        .map(|b| b.error.message)
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| format!("request failed (HTTP {status})"));
    ErrorKind::Fatal(CoreError::YouTube(message))
}

/// Search titles come HTML-escaped.
pub fn unescape_html(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub struct YouTubeClient {
    http: reqwest::Client,
    session: Arc<Session>,
}

impl YouTubeClient {
    pub fn new(http: reqwest::Client, session: Arc<Session>) -> Self {
        YouTubeClient { http, session }
    }

    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&Value>,
    ) -> CoreResult<Value> {
        let url = format!("{API_BASE}/{path}");
        let mut token = self.session.access_token().await?;
        let mut refreshed = false;
        let mut attempt = 0;
        loop {
            let mut req = self
                .http
                .request(method.clone(), &url)
                .query(query)
                .bearer_auth(&token);
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = req.send().await?;
            let status = resp.status().as_u16();
            if (200..300).contains(&status) {
                let bytes = resp.bytes().await?;
                return serde_json::from_slice(&bytes)
                    .map_err(|e| CoreError::YouTube(format!("unexpected response: {e}")));
            }
            if status == 401 && !refreshed {
                refreshed = true;
                token = self.session.refresh_after_401(&token).await?;
                continue;
            }
            if status == 401 {
                return Err(CoreError::SessionExpired);
            }
            let text = resp.text().await.unwrap_or_default();
            match classify_error(status, &text) {
                ErrorKind::QuotaExceeded => return Err(CoreError::YouTubeQuotaExceeded),
                ErrorKind::Retry if attempt < MAX_RETRIES => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                }
                ErrorKind::Retry => {
                    return Err(CoreError::YouTube(
                        "YouTube is busy right now; try again later".into(),
                    ))
                }
                ErrorKind::Fatal(e) => return Err(e),
            }
        }
    }

    /// The signed-in channel's name (1 unit).
    pub async fn channel_title(&self) -> CoreResult<String> {
        let v = self
            .call(
                reqwest::Method::GET,
                "channels",
                &[("part", "snippet"), ("mine", "true")],
                None,
            )
            .await?;
        Ok(v["items"][0]["snippet"]["title"]
            .as_str()
            .unwrap_or("YouTube account")
            .to_string())
    }
}

impl VideoApi for YouTubeClient {
    async fn search(&self, q: &str) -> CoreResult<Vec<SearchHit>> {
        let v = self
            .call(
                reqwest::Method::GET,
                "search",
                &[
                    ("part", "snippet"),
                    ("type", "video"),
                    ("maxResults", "10"),
                    ("q", q),
                ],
                None,
            )
            .await?;
        Ok(v["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|it| {
                        let id = it["id"]["videoId"].as_str()?;
                        Some(SearchHit {
                            video_id: id.to_string(),
                            title: unescape_html(it["snippet"]["title"].as_str().unwrap_or("")),
                            channel: unescape_html(
                                it["snippet"]["channelTitle"].as_str().unwrap_or(""),
                            ),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn durations(&self, ids: &[String]) -> CoreResult<HashMap<String, u64>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let joined = ids.iter().take(50).cloned().collect::<Vec<_>>().join(",");
        let v = self
            .call(
                reqwest::Method::GET,
                "videos",
                &[("part", "contentDetails"), ("id", &joined)],
                None,
            )
            .await?;
        let mut out = HashMap::new();
        for it in v["items"].as_array().into_iter().flatten() {
            if let (Some(id), Some(d)) = (
                it["id"].as_str(),
                it["contentDetails"]["duration"]
                    .as_str()
                    .and_then(parse_iso8601_duration),
            ) {
                out.insert(id.to_string(), d);
            }
        }
        Ok(out)
    }

    async fn create_playlist(&self, title: &str, description: &str) -> CoreResult<String> {
        let title: String = title.chars().take(150).collect();
        let body = json!({
            "snippet": { "title": title, "description": description },
            "status": { "privacyStatus": "private" }
        });
        let v = self
            .call(
                reqwest::Method::POST,
                "playlists",
                &[("part", "snippet,status")],
                Some(&body),
            )
            .await?;
        v["id"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| CoreError::YouTube("no playlist ID in the response".into()))
    }

    async fn insert_item(&self, playlist_id: &str, video_id: &str) -> CoreResult<()> {
        let body = json!({
            "snippet": {
                "playlistId": playlist_id,
                "resourceId": { "kind": "youtube#video", "videoId": video_id }
            }
        });
        self.call(
            reqwest::Method::POST,
            "playlistItems",
            &[("part", "snippet")],
            Some(&body),
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_exceeded_is_detected() {
        let body = r#"{"error":{"code":403,"message":"The request cannot be completed because you have exceeded your quota.","errors":[{"message":"x","domain":"youtube.quota","reason":"quotaExceeded"}]}}"#;
        assert_eq!(classify_error(403, body), ErrorKind::QuotaExceeded);
    }

    #[test]
    fn rate_limits_and_server_errors_retry() {
        let body = r#"{"error":{"code":403,"errors":[{"reason":"rateLimitExceeded"}]}}"#;
        assert_eq!(classify_error(403, body), ErrorKind::Retry);
        assert_eq!(classify_error(503, ""), ErrorKind::Retry);
    }

    #[test]
    fn other_errors_are_fatal_with_message() {
        let body = r#"{"error":{"code":404,"message":"Video not found.","errors":[{"reason":"videoNotFound"}]}}"#;
        assert_eq!(
            classify_error(404, body),
            ErrorKind::Fatal(CoreError::YouTube("Video not found.".into()))
        );
    }

    #[test]
    fn html_unescape() {
        assert_eq!(
            unescape_html("Rock &amp; Roll &#39;n&#39; &quot;x&quot;"),
            "Rock & Roll 'n' \"x\""
        );
    }
}
