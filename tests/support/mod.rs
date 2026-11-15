//! Fake UI states for rendering every screen in tests and for the README
//! screenshots. All names, IDs and URLs here are made up.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use playlist_porter::core::config::Config;
use playlist_porter::core::spotify::models::{Entry, TrackInfo};
use playlist_porter::gui::state::{
    Conn, DetailState, PlaylistRow, ResumeInfo, Screen, SignIn, UiError, UiState,
};

pub const ME: &str = "alex.example";

pub fn base_config() -> Config {
    Config {
        spotify_client_id: "0123456789abcdef0123456789abcdef".into(),
        google_client_id: "000000000000-example.apps.googleusercontent.com".into(),
        google_client_secret: "example-secret".into(),
        ..Default::default()
    }
}

fn url(kind: &str, id: &str) -> String {
    format!("https://open.spotify.com/{kind}/{id}")
}

fn row(
    id: &str,
    name: &str,
    owner: &str,
    collab: bool,
    total: Option<u32>,
    exportable: bool,
) -> PlaylistRow {
    PlaylistRow {
        id: id.into(),
        name: name.into(),
        owner_name: owner.into(),
        owner_is_me: owner == "Alex E.",
        collaborative: collab,
        total,
        spotify_url: url("playlist", id),
        exportable,
    }
}

pub fn playlists() -> Vec<PlaylistRow> {
    vec![
        row(
            "pl0000000000000000000A",
            "Coastal Drive",
            "Alex E.",
            false,
            Some(142),
            true,
        ),
        row(
            "pl0000000000000000000B",
            "Focus Blocks",
            "Alex E.",
            false,
            Some(87),
            true,
        ),
        row(
            "pl0000000000000000000C",
            "Weekend Kitchen",
            "Robin K.",
            true,
            Some(63),
            true,
        ),
        row(
            "pl0000000000000000000D",
            "Songs from that tiny record shop in Porto I keep meaning to revisit",
            "Alex E.",
            false,
            Some(211),
            true,
        ),
        row(
            "pl0000000000000000000E",
            "Quiet Acoustic Mornings",
            "sam.lee",
            false,
            None,
            false,
        ),
        row(
            "pl0000000000000000000F",
            "Stretching",
            "Alex E.",
            false,
            Some(0),
            true,
        ),
        row(
            "pl0000000000000000000G",
            "Party Requests",
            "jo.rivera",
            true,
            Some(118),
            true,
        ),
        row(
            "pl0000000000000000000H",
            "Rainy Day 90s",
            "Chris Doe",
            false,
            None,
            false,
        ),
    ]
}

fn track(n: usize, title: &str, artist: &str, album: &str, secs: u64, isrc: Option<&str>) -> Entry {
    Entry::Track(TrackInfo {
        id: format!("tr{n:020}"),
        title: title.into(),
        artists: vec![artist.into()],
        album: album.into(),
        duration_ms: secs * 1000,
        isrc: isrc.map(Into::into),
        spotify_url: url("track", &format!("tr{n:020}")),
    })
}

pub fn entries() -> Vec<Entry> {
    vec![
        track(
            1,
            "Harbour Lights - 2011 Remaster",
            "The Example Band",
            "Harbour Lights (Deluxe)",
            257,
            Some("XXA001100001"),
        ),
        track(
            2,
            "Open Road",
            "Northbound",
            "Mile Markers",
            218,
            Some("XXA001100002"),
        ),
        track(
            3,
            "City at Midnight",
            "Neon Atlas",
            "After Hours",
            243,
            Some("XXA001100003"),
        ),
        track(
            4,
            "Electric Tide",
            "Coral Static",
            "Low Frequencies",
            229,
            Some("XXA001100004"),
        ),
        track(
            5,
            "Mr. Sunshine",
            "The Weekenders",
            "Hot Summer",
            222,
            Some("XXA001100005"),
        ),
        track(
            6,
            "Still Moving",
            "Paper Lanterns",
            "Woodland",
            163,
            Some("XXA001100006"),
        ),
        Entry::Local {
            name: "roadtrip_mix_final.mp3".into(),
            duration_ms: 302_000,
        },
        track(
            8,
            "Country Miles",
            "J. Example",
            "Prairie Songs",
            193,
            Some("XXA001100008"),
        ),
        track(
            9,
            "Little Voices",
            "Of Owls and Men",
            "Inner Rooms",
            266,
            Some("XXA001100009"),
        ),
        Entry::Unavailable,
        track(11, "Holloway", "Fen & Field", "Fen & Field", 336, None),
        track(
            12,
            "Ride (feat. Kim Example)",
            "The Roadside Saints",
            "Highway Hymns, Vol. 2 – Extended Edition",
            291,
            Some("XXA001100012"),
        ),
    ]
}

pub fn signed_in(screen: Screen) -> UiState {
    UiState {
        screen,
        config: base_config(),
        file_config: base_config(),
        spotify: Conn::Connected {
            name: "Alex E.".into(),
            id: ME.into(),
        },
        youtube: Conn::Disconnected,
        list: playlists_list(),
        settings_port: "8888".into(),
        ..Default::default()
    }
}

fn playlists_list() -> playlist_porter::gui::state::ListState {
    playlist_porter::gui::state::ListState {
        playlists: playlists(),
        loaded_at: Instant::now().checked_sub(Duration::from_secs(125)),
        ..Default::default()
    }
}

pub fn setup() -> UiState {
    let mut s = UiState {
        screen: Screen::Setup,
        ..Default::default()
    };
    s.setup.spotify_client_id = "0123456789abcdef0123456789abcdef".into();
    s
}

pub fn signin_idle() -> UiState {
    UiState {
        screen: Screen::SignIn,
        config: base_config(),
        spotify: Conn::Disconnected,
        ..Default::default()
    }
}

pub fn signin_waiting() -> UiState {
    let mut s = signin_idle();
    s.spotify_signin = SignIn::Waiting {
        auth_url: "https://accounts.spotify.com/authorize?response_type=code&client_id=0123456789abcdef0123456789abcdef&state=EXAMPLE-STATE&code_challenge=EXAMPLE-CHALLENGE&code_challenge_method=S256&redirect_uri=http%3A%2F%2F127.0.0.1%3A8888%2Fcallback&scope=playlist-read-private+playlist-read-collaborative".into(),
        port: 8888,
        deadline: Instant::now() + Duration::from_secs(272),
    };
    s
}

pub fn signin_error() -> UiState {
    let mut s = signin_idle();
    s.signin_error = Some(UiError::from(
        playlist_porter::core::error::CoreError::SpotifyForbidden {
            spotify_message:
                "Check settings on developer.spotify.com/dashboard, the user may not be registered."
                    .into(),
        },
    ));
    s
}

pub fn main() -> UiState {
    let mut s = signed_in(Screen::Playlists);
    s.resumable.insert(
        "pl0000000000000000000B".into(),
        ResumeInfo { next_index: 31 },
    );
    s
}

pub fn main_filtered_empty() -> UiState {
    let mut s = signed_in(Screen::Playlists);
    s.list.filter = "zzz".into();
    s
}

pub fn detail() -> UiState {
    let mut s = signed_in(Screen::Detail);
    s.detail = Some(DetailState {
        row: playlists()[0].clone(),
        entries: Some(entries()),
        error: None,
    });
    s
}
