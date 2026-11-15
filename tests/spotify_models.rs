//! Spotify model, pagination and error-parsing tests. Offline: fixtures only.

use std::collections::HashMap;

use playlist_porter::core::error::CoreError;
use playlist_porter::core::spotify::client::{
    backoff_secs, collect_pages, ensure_exportable, is_api_url, is_exportable, is_valid_id,
    parse_429, RateLimit,
};
use playlist_porter::core::spotify::models::{
    api_error_message, Entry, Paging, Playlist, PlaylistEntry, SkipCounts, User,
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn entries() -> Vec<Entry> {
    let page: Paging<PlaylistEntry> =
        serde_json::from_str(&fixture("playlist_items.json")).unwrap();
    page.items.iter().map(Entry::from_api).collect()
}

#[test]
fn me_parses() {
    let u: User = serde_json::from_str(&fixture("me.json")).unwrap();
    assert_eq!(u.id, "user_a");
    assert_eq!(u.name(), "Alex Example");
}

#[test]
fn playlists_new_old_and_absent_items() {
    let p1: Paging<Playlist> = serde_json::from_str(&fixture("me_playlists_page1.json")).unwrap();
    let p2: Paging<Playlist> = serde_json::from_str(&fixture("me_playlists_page2.json")).unwrap();
    assert_eq!(p1.items[0].total(), Some(12)); // new `items`
    assert_eq!(p1.items[2].items(), None); // followed: absent
    assert_eq!(p1.items[2].owner.name(), "user_c"); // null display name falls back to id
    assert_eq!(p2.items[0].total(), Some(0)); // old `tracks`
}

#[test]
fn playlist_with_both_names_prefers_new() {
    let p: Playlist = serde_json::from_str(&fixture("playlist_both_names.json")).unwrap();
    assert_eq!(p.total(), Some(12));
}

#[test]
fn followed_playlist_has_no_items_and_is_not_exportable() {
    let p: Playlist = serde_json::from_str(&fixture("playlist_followed.json")).unwrap();
    assert!(p.items().is_none());
    assert_eq!(
        ensure_exportable(&p, "user_a"),
        Err(CoreError::NotExportable)
    );
}

#[test]
fn entry_shapes() {
    let e = entries();
    assert_eq!(e.len(), 7);
    let t0 = e[0].track().unwrap();
    assert_eq!(t0.title, "Dreams - 2004 Remaster");
    assert_eq!(t0.isrc.as_deref(), Some("USWB10400045"));
    assert_eq!(
        t0.spotify_url,
        "https://open.spotify.com/track/trackNew000000000000001"
    );
    // Old `track` name.
    let t1 = e[1].track().unwrap();
    assert_eq!(t1.artists, vec!["Artist Two", "Guest"]);
    assert_eq!(t1.isrc, None); // empty external_ids
                               // Both names present: new `item` wins.
    assert_eq!(e[2].track().unwrap().title, "Both Names (new wins)");
    assert_eq!(e[3], Entry::Unavailable); // null item
    assert!(matches!(&e[4], Entry::Episode { name } if name == "A Podcast Episode"));
    assert!(matches!(&e[5], Entry::Local { name, .. } if name == "roadtrip_mix_final.mp3"));
    // external_ids missing entirely.
    assert_eq!(e[6].track().unwrap().isrc, None);
}

#[test]
fn skip_counts() {
    let c = SkipCounts::of(&entries());
    assert_eq!((c.local, c.episode, c.unavailable), (1, 1, 1));
    assert_eq!(c.total(), 3);
    assert_eq!(
        c.describe(),
        "1 local file, 1 podcast episode, 1 unavailable track"
    );
}

#[test]
fn exportability_rules() {
    assert!(is_exportable("me", false, "me"));
    assert!(is_exportable("other", true, "me"));
    assert!(!is_exportable("other", false, "me"));
}

#[test]
fn id_validation_blocks_path_and_url_tricks() {
    assert!(is_valid_id("1AbCdEfGhIjKlMnOpQrStU"));
    assert!(!is_valid_id("../etc"));
    assert!(!is_valid_id("abc/def"));
    assert!(!is_valid_id(""));
}

#[tokio::test]
async fn pagination_follows_next() {
    let mut pages: HashMap<String, String> = HashMap::new();
    pages.insert(
        "https://api.spotify.com/v1/me/playlists?limit=50".into(),
        fixture("me_playlists_page1.json"),
    );
    pages.insert(
        "https://api.spotify.com/v1/me/playlists?offset=50&limit=50".into(),
        fixture("me_playlists_page2.json"),
    );
    let mut seen = Vec::new();
    let all = collect_pages(
        "https://api.spotify.com/v1/me/playlists?limit=50".to_string(),
        |u| {
            seen.push(u.clone());
            let body = pages[&u].clone();
            async move { Ok(serde_json::from_str::<Paging<Playlist>>(&body).unwrap()) }
        },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(seen.len(), 2);
}

#[tokio::test]
async fn pagination_refuses_foreign_next() {
    let res = collect_pages::<Playlist, _, _>(
        "https://api.spotify.com/v1/me/playlists?limit=50".to_string(),
        |_| async {
            Ok(Paging {
                items: vec![],
                next: Some("https://evil.example/steal".into()),
                total: None,
            })
        },
    )
    .await;
    assert!(res.is_err());
    assert!(is_api_url("https://api.spotify.com/v1/me"));
    assert!(!is_api_url("https://api.spotify.com.evil.example/v1/me"));
}

#[test]
fn rate_limit_parsing() {
    assert_eq!(parse_429(Some("7"), ""), RateLimit::RetryAfter(7));
    assert_eq!(parse_429(None, "{}"), RateLimit::RetryAfter(1));
    assert_eq!(parse_429(Some("0"), ""), RateLimit::RetryAfter(1));
    assert_eq!(parse_429(Some("garbage"), ""), RateLimit::RetryAfter(1));
    assert_eq!(
        parse_429(Some("30"), &fixture("error_429_quota.json")),
        RateLimit::QuotaExceeded
    );
    assert_eq!(backoff_secs(7, 0), 7);
    assert_eq!(backoff_secs(1, 3), 8);
}

#[test]
fn forbidden_message_is_verbatim() {
    let msg = api_error_message(&fixture("error_403.json"));
    assert!(msg.starts_with("Check settings on developer.spotify.com"));
    let err = CoreError::SpotifyForbidden {
        spotify_message: msg.clone(),
    };
    let s = err.to_string();
    assert!(s.starts_with("Spotify refused access."));
    assert!(s.contains("User Management"));
    assert!(s.contains(&msg));
    assert!(err.wants_setup_link());
}

// --- Liked Songs ------------------------------------------------------------

use playlist_porter::core::auth::StoredToken;
use playlist_porter::core::spotify::client::forbidden_error;
use playlist_porter::core::spotify::models::{LIKED_SONGS_ID, LIKED_SONGS_URL};

#[test]
fn saved_tracks_parse_as_entries() {
    let page: Paging<PlaylistEntry> = serde_json::from_str(&fixture("saved_tracks.json")).unwrap();
    assert_eq!(page.total, Some(3));
    let e: Vec<Entry> = page.items.iter().map(Entry::from_api).collect();
    assert_eq!(e[0].track().unwrap().title, "Saved One");
    assert_eq!(
        e[1].track().unwrap().spotify_url,
        "https://open.spotify.com/track/savedTrack000000000002"
    );
    assert_eq!(e[2], Entry::Unavailable);
}

#[test]
fn liked_songs_is_an_exportable_owned_playlist() {
    let me: User = serde_json::from_str(&fixture("me.json")).unwrap();
    let p = Playlist::liked_songs(&me, Some(3));
    assert_eq!(p.id, LIKED_SONGS_ID);
    assert!(p.is_liked_songs());
    assert_eq!(p.total(), Some(3));
    assert_eq!(p.spotify_url(), LIKED_SONGS_URL);
    assert!(ensure_exportable(&p, &me.id).is_ok());
    // Usable as a resume-file name and never a real 22-character playlist ID.
    assert!(is_valid_id(LIKED_SONGS_ID));
    assert_ne!(LIKED_SONGS_ID.len(), 22);
}

#[test]
fn missing_scope_403_asks_to_reconnect() {
    let msg = api_error_message(&fixture("error_403_scope.json"));
    assert_eq!(forbidden_error(&msg), CoreError::ReauthNeeded);
    let other = api_error_message(&fixture("error_403.json"));
    assert!(matches!(
        forbidden_error(&other),
        CoreError::SpotifyForbidden { .. }
    ));
}

#[test]
fn old_tokens_without_library_scope_need_reauth() {
    let scopes = playlist_porter::core::auth::spotify::SCOPES;
    let mut t = StoredToken {
        access_token: "a".into(),
        refresh_token: None,
        expires_at: 0,
        scope: None,
    };
    assert!(!t.has_scopes(&scopes)); // saved before scopes were recorded
    t.scope = Some("playlist-read-private playlist-read-collaborative".into());
    assert!(!t.has_scopes(&scopes));
    t.scope = Some("user-library-read playlist-read-collaborative playlist-read-private".into());
    assert!(t.has_scopes(&scopes));
    // Old keyring JSON (no `scope` key) still parses.
    let old: StoredToken =
        serde_json::from_str(r#"{"access_token":"a","refresh_token":"r","expires_at":1}"#).unwrap();
    assert_eq!(old.scope, None);
}
