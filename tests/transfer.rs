//! Transfer loop against a fake YouTube API: full run, quota stop,
//! quotaExceeded from the API, cancellation and resume. Offline.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use playlist_porter::core::error::{CoreError, CoreResult};
use playlist_porter::core::matching::MatchStatus;
use playlist_porter::core::spotify::models::TrackInfo;
use playlist_porter::core::youtube::quota::QuotaTracker;
use playlist_porter::core::youtube::resume::{ResumeState, ResumeStore, TrackResult};
use playlist_porter::core::youtube::transfer::{
    playlist_description, run, Outcome, Progress, SearchHit, TransferJob, VideoApi,
};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Fake {
    log: Mutex<Vec<String>>,
    quota_fail_on_insert: bool,
}

impl Fake {
    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl VideoApi for Fake {
    async fn search(&self, q: &str) -> CoreResult<Vec<SearchHit>> {
        self.log.lock().unwrap().push(format!("search:{q}"));
        // "Artist Title" → an exact topic-channel hit for that title.
        let (artist, title) = q.split_once(' ').unwrap();
        Ok(vec![SearchHit {
            video_id: format!("v-{title}"),
            title: title.to_string(),
            channel: format!("{artist} - Topic"),
        }])
    }

    async fn durations(&self, ids: &[String]) -> CoreResult<HashMap<String, u64>> {
        self.log.lock().unwrap().push("videos".into());
        Ok(ids.iter().map(|i| (i.clone(), 200)).collect())
    }

    async fn create_playlist(&self, title: &str, description: &str) -> CoreResult<String> {
        self.log
            .lock()
            .unwrap()
            .push(format!("create:{title}|{description}"));
        Ok("PLnew".into())
    }

    async fn insert_item(&self, playlist_id: &str, video_id: &str) -> CoreResult<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("insert:{playlist_id}:{video_id}"));
        if self.quota_fail_on_insert {
            return Err(CoreError::YouTubeQuotaExceeded);
        }
        Ok(())
    }
}

fn track(n: usize) -> TrackInfo {
    TrackInfo {
        id: format!("t{n}"),
        title: format!("Song{n}"),
        artists: vec![format!("Artist{n}")],
        album: "A".into(),
        duration_ms: 200_000,
        isrc: None,
        spotify_url: format!("https://open.spotify.com/track/t{n}"),
    }
}

fn job(n: usize, resume: Option<ResumeState>) -> TransferJob {
    TransferJob {
        spotify_playlist_id: "plTEST1".into(),
        playlist_name: "My List".into(),
        playlist_url: "https://open.spotify.com/playlist/plTEST1".into(),
        tracks: (0..n).map(track).collect(),
        resume,
    }
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-14T20:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn store(tag: &str) -> ResumeStore {
    ResumeStore::new(std::env::temp_dir().join(format!("pp-transfer-{tag}-{}", std::process::id())))
}

#[tokio::test]
async fn full_run_creates_private_playlist_and_matches() {
    let api = Fake::default();
    let quota = Mutex::new(QuotaTracker::new(now()));
    let st = store("full");
    let mut rows = Vec::new();
    let out = run(
        &api,
        job(3, None),
        &quota,
        Some(&st),
        &CancellationToken::new(),
        |p| {
            if let Progress::Row(r) = p {
                rows.push(r)
            }
        },
        now,
    )
    .await;
    assert_eq!(
        out,
        Outcome::Done {
            youtube_playlist_id: "PLnew".into()
        }
    );
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|r| r.status == MatchStatus::Matched));
    assert_eq!(rows[0].spotify_url, "https://open.spotify.com/track/t0");
    let log = api.log();
    assert_eq!(
        log[0],
        format!(
            "create:My List|{}",
            playlist_description("https://open.spotify.com/playlist/plTEST1")
        )
    );
    assert!(log[0]
        .ends_with("Track list supplied by Spotify: https://open.spotify.com/playlist/plTEST1"));
    assert_eq!(
        log.iter().filter(|l| l.starts_with("insert:PLnew")).count(),
        3
    );
    // 50 + 3 × (100 + 1 + 50)
    assert_eq!(quota.lock().unwrap().used(now()), 503);
    // Resume state is deleted when the run finishes.
    assert_eq!(st.load("plTEST1", SystemTime::now()).unwrap(), None);
}

#[tokio::test]
async fn stops_before_running_out_of_quota_and_saves_state() {
    let api = Fake::default();
    let quota = Mutex::new(QuotaTracker::new(now()));
    quota.lock().unwrap().spend(9_700, now());
    let st = store("quota");
    let out = run(
        &api,
        job(3, None),
        &quota,
        Some(&st),
        &CancellationToken::new(),
        |_| {},
        now,
    )
    .await;
    // 50 for the playlist + 151 for one track fit; the second track doesn't.
    assert_eq!(
        out,
        Outcome::QuotaExhausted {
            youtube_playlist_id: Some("PLnew".into())
        }
    );
    let saved = st.load("plTEST1", SystemTime::now()).unwrap().unwrap();
    assert_eq!(saved.youtube_playlist_id, "PLnew");
    assert_eq!(saved.next_index, 1);
    assert_eq!(saved.results.len(), 1);
    st.delete_all().unwrap();
}

#[tokio::test]
async fn quota_exceeded_from_api_stops_cleanly() {
    let api = Fake {
        quota_fail_on_insert: true,
        ..Default::default()
    };
    let quota = Mutex::new(QuotaTracker::new(now()));
    let st = store("qe");
    let out = run(
        &api,
        job(2, None),
        &quota,
        Some(&st),
        &CancellationToken::new(),
        |_| {},
        now,
    )
    .await;
    assert!(matches!(out, Outcome::QuotaExhausted { .. }));
    assert_eq!(quota.lock().unwrap().remaining(now()), 0);
    let saved = st.load("plTEST1", SystemTime::now()).unwrap().unwrap();
    // The interrupted track is redone after the reset.
    assert_eq!(saved.next_index, 0);
    st.delete_all().unwrap();
}

#[tokio::test]
async fn cancel_stops_after_current_call_and_saves() {
    let api = Fake::default();
    let quota = Mutex::new(QuotaTracker::new(now()));
    let st = store("cancel");
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    let out = run(
        &api,
        job(5, None),
        &quota,
        Some(&st),
        &cancel,
        |p| {
            if matches!(p, Progress::Row(_)) {
                c2.cancel();
            }
        },
        now,
    )
    .await;
    assert!(matches!(out, Outcome::Cancelled { .. }));
    let saved = st.load("plTEST1", SystemTime::now()).unwrap().unwrap();
    assert_eq!(saved.next_index, 1);
    st.delete_all().unwrap();
}

#[tokio::test]
async fn resume_continues_where_it_stopped() {
    let api = Fake::default();
    let quota = Mutex::new(QuotaTracker::new(now()));
    let st = store("resume");
    let resume = ResumeState {
        spotify_playlist_id: "plTEST1".into(),
        youtube_playlist_id: "PLold".into(),
        next_index: 2,
        results: vec![
            TrackResult {
                video_id: Some("v-Song0".into()),
                status: MatchStatus::Matched,
            },
            TrackResult {
                video_id: None,
                status: MatchStatus::NotFound,
            },
        ],
    };
    let mut rows = Vec::new();
    let out = run(
        &api,
        job(3, Some(resume)),
        &quota,
        Some(&st),
        &CancellationToken::new(),
        |p| {
            if let Progress::Row(r) = p {
                rows.push(r)
            }
        },
        now,
    )
    .await;
    assert_eq!(
        out,
        Outcome::Done {
            youtube_playlist_id: "PLold".into()
        }
    );
    let log = api.log();
    assert!(!log.iter().any(|l| l.starts_with("create:")));
    assert_eq!(log.iter().filter(|l| l.starts_with("search:")).count(), 1);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].status, MatchStatus::NotFound);
    // Restored rows still carry the (re-fetched) Spotify URL.
    assert_eq!(rows[0].spotify_url, "https://open.spotify.com/track/t0");
}
