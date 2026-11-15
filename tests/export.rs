//! CSV/JSON exports carry attribution and per-track Spotify URLs.

use playlist_porter::core::config::ExportFormat;
use playlist_porter::core::error::CoreError;
use playlist_porter::core::export::{export_playlist, to_csv, to_json, PlaylistMeta, ATTRIBUTION};
use playlist_porter::core::spotify::models::{Entry, Paging, Playlist, PlaylistEntry};

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

fn meta() -> PlaylistMeta {
    PlaylistMeta {
        name: "Owned, new field names".into(),
        url: "https://open.spotify.com/playlist/1AbCdEfGhIjKlMnOpQrStU".into(),
    }
}

#[test]
fn csv_has_attribution_comment_then_header_and_urls() {
    let csv = to_csv(&meta(), &entries()).unwrap();
    let mut lines = csv.lines();
    let first = lines.next().unwrap();
    assert!(first.starts_with("# "));
    assert!(first.contains(ATTRIBUTION));
    assert!(first.contains("https://open.spotify.com/playlist/1AbCdEfGhIjKlMnOpQrStU"));
    assert_eq!(
        lines.next().unwrap(),
        "position,title,artists,album,duration_ms,isrc,spotify_url"
    );
    let rows: Vec<&str> = lines.collect();
    // 7 entries, 3 skipped (null, episode, local).
    assert_eq!(rows.len(), 4);
    for r in &rows {
        assert!(r.contains("https://open.spotify.com/track/"), "{r}");
    }
    assert!(rows[1].contains("Artist Two; Guest"));
    assert!(!csv.contains("roadtrip_mix_final.mp3"));
    assert!(!csv.contains("A Podcast Episode"));
}

#[test]
fn csv_comment_cannot_be_broken_by_the_name() {
    let m = PlaylistMeta {
        name: "evil\nname,with\"quotes".into(),
        url: meta().url,
    };
    let csv = to_csv(&m, &entries()).unwrap();
    assert!(csv.lines().nth(1).unwrap().starts_with("position,"));
}

#[test]
fn json_has_attribution_playlist_url_and_track_urls() {
    let v: serde_json::Value =
        serde_json::from_str(&to_json(&meta(), &entries()).unwrap()).unwrap();
    assert_eq!(v["attribution"], ATTRIBUTION);
    assert_eq!(
        v["playlist_url"],
        "https://open.spotify.com/playlist/1AbCdEfGhIjKlMnOpQrStU"
    );
    let tracks = v["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 4);
    assert!(tracks.iter().all(|t| t["spotify_url"]
        .as_str()
        .unwrap()
        .starts_with("https://open.spotify.com/track/")));
    assert_eq!(tracks[0]["isrc"], "USWB10400045");
    assert!(tracks[1]["isrc"].is_null());
    assert_eq!(v["skipped"]["local_files"], 1);
    assert_eq!(v["skipped"]["episodes"], 1);
    assert_eq!(v["skipped"]["unavailable"], 1);
}

#[test]
fn export_rechecks_ownership() {
    let p: Playlist = serde_json::from_str(&fixture("playlist_followed.json")).unwrap();
    let path = std::env::temp_dir().join("pp-should-not-exist.csv");
    let res = export_playlist(&p, &entries(), "user_a", ExportFormat::Csv, &path, false);
    assert_eq!(res, Err(CoreError::NotExportable));
    assert!(!path.exists());
}

#[test]
fn export_writes_owned_playlist() {
    let p: Playlist = serde_json::from_str(&fixture("playlist_both_names.json")).unwrap();
    let dir = std::env::temp_dir().join(format!("pp-export-it-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.json");
    let n = export_playlist(&p, &entries(), "user_a", ExportFormat::Json, &path, false).unwrap();
    assert_eq!(n, 4);
    assert!(std::fs::read_to_string(&path)
        .unwrap()
        .contains(ATTRIBUTION));
    std::fs::remove_dir_all(&dir).unwrap();
}
