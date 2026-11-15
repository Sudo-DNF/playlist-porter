//! The transfer loop: create a private YouTube playlist and add the best
//! match for each track, with quota checks, resume state and cancellation.
//!
//! Generic over [`VideoApi`] so it can be tested offline with a fake.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;

use super::quota::{
    QuotaTracker, COST_ITEM_INSERT, COST_PER_TRACK, COST_PLAYLIST_INSERT, COST_SEARCH, COST_VIDEOS,
};
use super::report::ReportRow;
use super::resume::{ResumeState, ResumeStore, TrackResult};
use crate::core::error::{CoreError, CoreResult};
use crate::core::matching::{self, Candidate, MatchStatus, TrackQuery};
use crate::core::spotify::models::TrackInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub video_id: String,
    pub title: String,
    pub channel: String,
}

/// The YouTube calls a transfer needs.
pub trait VideoApi: Send + Sync {
    fn search(&self, q: &str) -> impl Future<Output = CoreResult<Vec<SearchHit>>> + Send;
    fn durations(
        &self,
        ids: &[String],
    ) -> impl Future<Output = CoreResult<HashMap<String, u64>>> + Send;
    fn create_playlist(
        &self,
        title: &str,
        description: &str,
    ) -> impl Future<Output = CoreResult<String>> + Send;
    fn insert_item(
        &self,
        playlist_id: &str,
        video_id: &str,
    ) -> impl Future<Output = CoreResult<()>> + Send;
}

/// "Track list supplied by Spotify: <url>"
pub fn playlist_description(spotify_playlist_url: &str) -> String {
    format!("Track list supplied by Spotify: {spotify_playlist_url}")
}

pub struct TransferJob {
    pub spotify_playlist_id: String,
    pub playlist_name: String,
    pub playlist_url: String,
    /// Only real tracks (local files, episodes and unavailable items removed),
    /// freshly fetched from Spotify.
    pub tracks: Vec<TrackInfo>,
    pub resume: Option<ResumeState>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    PlaylistReady(String),
    Current {
        index: usize,
        total: usize,
        label: String,
    },
    Row(ReportRow),
    QuotaUsed(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Done {
        youtube_playlist_id: String,
    },
    QuotaExhausted {
        youtube_playlist_id: Option<String>,
    },
    Cancelled {
        youtube_playlist_id: Option<String>,
    },
    Failed {
        error: CoreError,
        youtube_playlist_id: Option<String>,
    },
}

fn base_row(i: usize, t: &TrackInfo) -> ReportRow {
    ReportRow {
        position: i + 1,
        title: t.title.clone(),
        artists: t.artists_joined(),
        spotify_url: t.spotify_url.clone(),
        status: MatchStatus::NotFound,
        video_id: None,
        video_title: None,
        channel: None,
        duration_diff_secs: None,
        note: None,
    }
}

struct Ctx<'a, F: FnMut(Progress), N: Fn() -> DateTime<Utc>> {
    quota: &'a Mutex<QuotaTracker>,
    store: Option<&'a ResumeStore>,
    on: F,
    now: N,
}

impl<F: FnMut(Progress), N: Fn() -> DateTime<Utc>> Ctx<'_, F, N> {
    fn can_afford(&self, cost: u32) -> bool {
        self.quota
            .lock()
            .map(|mut q| q.can_afford(cost, (self.now)()))
            .unwrap_or(false)
    }

    fn spend(&mut self, cost: u32) {
        let used = self
            .quota
            .lock()
            .map(|mut q| {
                q.spend(cost, (self.now)());
                q.used((self.now)())
            })
            .unwrap_or(0);
        (self.on)(Progress::QuotaUsed(used));
    }

    fn exhaust(&mut self) {
        if let Ok(mut q) = self.quota.lock() {
            q.exhaust((self.now)());
        }
        (self.on)(Progress::QuotaUsed(super::quota::DAILY_LIMIT));
    }

    fn save(&self, state: &ResumeState) {
        if let Some(s) = self.store {
            // Best effort: a failed save shouldn't abort the transfer.
            let _ = s.save(state);
        }
    }
}

fn yt_id(state: &ResumeState) -> Option<String> {
    (!state.youtube_playlist_id.is_empty()).then(|| state.youtube_playlist_id.clone())
}

/// Run (or resume) a transfer. Cancellation is checked between API calls,
/// so a cancel stops after the current call and saves resume state.
pub async fn run<A: VideoApi>(
    api: &A,
    job: TransferJob,
    quota: &Mutex<QuotaTracker>,
    store: Option<&ResumeStore>,
    cancel: &CancellationToken,
    on: impl FnMut(Progress),
    now: impl Fn() -> DateTime<Utc>,
) -> Outcome {
    let mut cx = Ctx {
        quota,
        store,
        on,
        now,
    };
    let total = job.tracks.len();
    let mut state = job.resume.unwrap_or_else(|| ResumeState {
        spotify_playlist_id: job.spotify_playlist_id.clone(),
        youtube_playlist_id: String::new(),
        next_index: 0,
        results: Vec::new(),
    });
    // The playlist may have changed since the state was saved.
    state.next_index = state.next_index.min(total).min(state.results.len());
    state.results.truncate(state.next_index);

    // Rows restored from earlier sessions (video IDs only were saved).
    for (i, r) in state.results.iter().enumerate() {
        let mut row = base_row(i, &job.tracks[i]);
        row.status = r.status;
        row.video_id = r.video_id.clone();
        row.note = Some("Added in an earlier session".into());
        (cx.on)(Progress::Row(row));
    }

    macro_rules! stop {
        ($outcome:expr) => {{
            cx.save(&state);
            return $outcome;
        }};
    }

    if state.youtube_playlist_id.is_empty() {
        if cancel.is_cancelled() {
            return Outcome::Cancelled {
                youtube_playlist_id: None,
            };
        }
        if !cx.can_afford(COST_PLAYLIST_INSERT) {
            return Outcome::QuotaExhausted {
                youtube_playlist_id: None,
            };
        }
        cx.spend(COST_PLAYLIST_INSERT);
        match api
            .create_playlist(&job.playlist_name, &playlist_description(&job.playlist_url))
            .await
        {
            Ok(id) => {
                state.youtube_playlist_id = id.clone();
                cx.save(&state);
                (cx.on)(Progress::PlaylistReady(id));
            }
            Err(CoreError::YouTubeQuotaExceeded) => {
                cx.exhaust();
                return Outcome::QuotaExhausted {
                    youtube_playlist_id: None,
                };
            }
            Err(error) => {
                return Outcome::Failed {
                    error,
                    youtube_playlist_id: None,
                };
            }
        }
    } else {
        (cx.on)(Progress::PlaylistReady(state.youtube_playlist_id.clone()));
    }

    for i in state.next_index..total {
        let t = &job.tracks[i];
        if cancel.is_cancelled() {
            stop!(Outcome::Cancelled {
                youtube_playlist_id: yt_id(&state)
            });
        }
        if !cx.can_afford(COST_PER_TRACK) {
            stop!(Outcome::QuotaExhausted {
                youtube_playlist_id: yt_id(&state)
            });
        }
        (cx.on)(Progress::Current {
            index: i,
            total,
            label: format!("{}, {}", t.title, t.primary_artist()),
        });

        // search.list
        cx.spend(COST_SEARCH);
        let hits = match api
            .search(&matching::build_query(t.primary_artist(), &t.title))
            .await
        {
            Ok(h) => h,
            Err(CoreError::YouTubeQuotaExceeded) => {
                cx.exhaust();
                stop!(Outcome::QuotaExhausted {
                    youtube_playlist_id: yt_id(&state)
                });
            }
            Err(error) => stop!(Outcome::Failed {
                error,
                youtube_playlist_id: yt_id(&state)
            }),
        };
        if cancel.is_cancelled() {
            stop!(Outcome::Cancelled {
                youtube_playlist_id: yt_id(&state)
            });
        }

        // videos.list for durations
        let ids: Vec<String> = hits.iter().map(|h| h.video_id.clone()).collect();
        let durations = if ids.is_empty() {
            HashMap::new()
        } else {
            cx.spend(COST_VIDEOS);
            match api.durations(&ids).await {
                Ok(d) => d,
                Err(CoreError::YouTubeQuotaExceeded) => {
                    cx.exhaust();
                    stop!(Outcome::QuotaExhausted {
                        youtube_playlist_id: yt_id(&state)
                    });
                }
                Err(error) => stop!(Outcome::Failed {
                    error,
                    youtube_playlist_id: yt_id(&state)
                }),
            }
        };
        let candidates: Vec<Candidate> = hits
            .into_iter()
            .map(|h| Candidate {
                duration_secs: durations.get(&h.video_id).copied(),
                video_id: h.video_id,
                title: h.title,
                channel: h.channel,
            })
            .collect();
        let outcome = matching::choose(
            &TrackQuery {
                title: &t.title,
                artists: &t.artists,
                duration_ms: t.duration_ms,
            },
            &candidates,
        );

        let mut row = base_row(i, t);
        row.status = outcome.status;
        row.note = outcome.note.clone();
        if let Some(b) = &outcome.best {
            row.video_id = Some(b.candidate.video_id.clone());
            row.video_title = Some(b.candidate.title.clone());
            row.channel = Some(b.candidate.channel.clone());
            row.duration_diff_secs = b.duration_diff_secs;
        }

        // playlistItems.insert
        if outcome.status.is_added() {
            if cancel.is_cancelled() {
                stop!(Outcome::Cancelled {
                    youtube_playlist_id: yt_id(&state)
                });
            }
            let vid = row.video_id.clone().unwrap_or_default();
            cx.spend(COST_ITEM_INSERT);
            match api.insert_item(&state.youtube_playlist_id, &vid).await {
                Ok(()) => {}
                Err(CoreError::YouTubeQuotaExceeded) => {
                    cx.exhaust();
                    // This track is redone after the reset.
                    stop!(Outcome::QuotaExhausted {
                        youtube_playlist_id: yt_id(&state)
                    });
                }
                Err(CoreError::YouTube(msg)) => {
                    // e.g. the video can't be added to playlists.
                    row.status = MatchStatus::NotFound;
                    row.note = Some(format!("Couldn't add: {msg}"));
                }
                Err(error) => stop!(Outcome::Failed {
                    error,
                    youtube_playlist_id: yt_id(&state)
                }),
            }
        }

        state.results.push(TrackResult {
            video_id: row
                .status
                .is_added()
                .then(|| row.video_id.clone())
                .flatten(),
            status: row.status,
        });
        state.next_index = i + 1;
        cx.save(&state);
        (cx.on)(Progress::Row(row));
    }

    if let Some(s) = cx.store {
        let _ = s.delete(&state.spotify_playlist_id);
    }
    Outcome::Done {
        youtube_playlist_id: state.youtube_playlist_id,
    }
}
