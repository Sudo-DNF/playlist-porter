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

fn export_dialog(step: playlist_porter::gui::state::ExportStep) -> UiState {
    use playlist_porter::core::spotify::models::SkipCounts;
    use playlist_porter::gui::state::ExportDialog;
    let mut s = signed_in(Screen::Playlists);
    s.export = Some(ExportDialog {
        playlist_id: "pl0000000000000000000A".into(),
        playlist_name: "Coastal Drive".into(),
        format: Default::default(),
        path: "/home/alex/Documents/Coastal Drive.csv".into(),
        step,
        track_count: 140,
        skipped: SkipCounts {
            local: 1,
            episode: 0,
            unavailable: 1,
        },
        error: None,
    });
    s
}

pub fn export_form() -> UiState {
    export_dialog(playlist_porter::gui::state::ExportStep::Form)
}

pub fn export_overwrite() -> UiState {
    export_dialog(playlist_porter::gui::state::ExportStep::ConfirmOverwrite)
}

pub fn export_done_toast() -> UiState {
    let mut s = signed_in(Screen::Playlists);
    s.toast(
        playlist_porter::gui::state::ToastKind::Success,
        "Exported 140 tracks to Coastal Drive.csv",
        Some("/home/alex/Documents".into()),
    );
    s
}

// --- YouTube transfer -------------------------------------------------------

use playlist_porter::core::matching::MatchStatus;
use playlist_porter::core::youtube::report::ReportRow;
use playlist_porter::gui::transfer_state::{ConfirmInfo, TransferPhase, TransferView};

fn transfer_view(phase: TransferPhase) -> TransferView {
    TransferView::new(
        "pl0000000000000000000B",
        "Focus Blocks",
        "https://open.spotify.com/playlist/pl0000000000000000000B",
        false,
        phase,
    )
}

fn yt_connected(screen: Screen) -> UiState {
    let mut s = signed_in(screen);
    s.youtube = Conn::Connected {
        name: "Alex E.".into(),
        id: String::new(),
    };
    s.quota_reset_local = "3:00 AM".into();
    s
}

pub fn yt_notice() -> UiState {
    let mut s = signed_in(Screen::Transfers);
    s.transfer = Some(transfer_view(TransferPhase::Notice));
    s
}

pub fn yt_signing_in() -> UiState {
    let mut s = signed_in(Screen::Transfers);
    s.transfer = Some(transfer_view(TransferPhase::SigningIn));
    s.google_signin = SignIn::Waiting {
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id=000000000000-example.apps.googleusercontent.com&state=EXAMPLE&code_challenge=EXAMPLE&code_challenge_method=S256&redirect_uri=http%3A%2F%2F127.0.0.1%3A8888%2Fcallback&scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fyoutube&access_type=offline&prompt=consent".into(),
        port: 8888,
        deadline: Instant::now() + Duration::from_secs(290),
    };
    s
}

pub fn yt_confirm() -> UiState {
    let mut s = yt_connected(Screen::Playlists);
    let mut t = transfer_view(TransferPhase::Confirm);
    t.confirm = Some(ConfirmInfo {
        tracks: 87,
        total_tracks: 87,
        skipped: Default::default(),
        units: 13_100,
        days: 2,
        reset_local: "3:00 AM".into(),
        resuming: false,
    });
    s.transfer = Some(t);
    s
}

fn rrow(
    n: usize,
    title: &str,
    artist: &str,
    status: MatchStatus,
    video: Option<(&str, &str)>,
    diff: Option<i64>,
    note: Option<&str>,
) -> ReportRow {
    ReportRow {
        position: n,
        title: title.into(),
        artists: artist.into(),
        spotify_url: format!("https://open.spotify.com/track/tr{n:020}"),
        status,
        video_id: video.map(|_| format!("vid{n:08}")),
        video_title: video.map(|v| v.0.to_string()),
        channel: video.map(|v| v.1.to_string()),
        duration_diff_secs: diff,
        note: note.map(Into::into),
    }
}

fn report_rows() -> Vec<ReportRow> {
    use MatchStatus::*;
    vec![
        rrow(
            20,
            "Opening",
            "Neon Atlas",
            Matched,
            Some(("Opening", "Neon Atlas - Topic")),
            Some(0),
            None,
        ),
        rrow(
            21,
            "Tidal",
            "Coral Static",
            Matched,
            Some(("Coral Static - Tidal", "CoralStaticVEVO")),
            Some(1),
            None,
        ),
        rrow(
            22,
            "Night Call",
            "Paper Lanterns",
            Matched,
            Some(("Night Call", "Paper Lanterns - Topic")),
            Some(0),
            None,
        ),
        rrow(
            23,
            "Long Ending (Ascent) - Remastered 2005",
            "Ambient Person",
            Uncertain,
            Some((
                "Ambient Person - Long Ending (Ascent) [1 Hour]",
                "Ambient Archive",
            )),
            Some(3334),
            Some("Duration far off"),
        ),
        rrow(
            24,
            "Window Light",
            "Fen & Field",
            Matched,
            Some(("Fen & Field - Window Light", "Example Records")),
            Some(-2),
            None,
        ),
        rrow(
            25,
            "Morning Bells",
            "Northbound",
            Uncertain,
            Some(("northbound morning bells hq", "uploader77")),
            Some(3),
            Some("Low title similarity"),
        ),
        rrow(
            26,
            "Gentle",
            "The Weekenders",
            Matched,
            Some((
                "The Weekenders - Gentle (Official Video)",
                "TheWeekendersVEVO",
            )),
            Some(4),
            None,
        ),
        rrow(
            27,
            "Midnight Session (feat. Kim Example)",
            "Nightdesk",
            NotFound,
            None,
            None,
            Some("No usable result"),
        ),
        rrow(
            28,
            "Strobe Light",
            "J. Example",
            Uncertain,
            Some(("J. Example - Strobe Light (Radio Edit)", "J. Example")),
            Some(-372),
            Some("Duration far off"),
        ),
        rrow(
            29,
            "Spectrum",
            "Of Owls and Men",
            Matched,
            Some(("Spectrum", "Of Owls and Men - Topic")),
            Some(0),
            None,
        ),
        rrow(
            30,
            "Porcelain Sky",
            "The Example Band",
            Matched,
            Some((
                "The Example Band - Porcelain Sky (Official Video)",
                "The Example Band",
            )),
            Some(3),
            None,
        ),
    ]
}

fn transfer_with(phase: TransferPhase, done: usize) -> UiState {
    let mut s = yt_connected(Screen::Transfers);
    let mut t = transfer_view(phase);
    t.total = 87;
    t.done = done;
    t.current = Some("#32 Holloway, Fen & Field".into());
    t.rows = report_rows();
    t.youtube_playlist_id = Some("PLexample0000000000000000000000000".into());
    s.transfer = Some(t);
    s.quota_used = 4_700;
    s
}

pub fn transfer_running() -> UiState {
    transfer_with(TransferPhase::Running, 31)
}

pub fn transfer_quota() -> UiState {
    let mut s = transfer_with(TransferPhase::QuotaExhausted, 65);
    s.quota_used = 10_000;
    s
}

pub fn transfer_cancelled() -> UiState {
    transfer_with(TransferPhase::Cancelled, 31)
}

pub fn transfer_done() -> UiState {
    let mut s = transfer_with(TransferPhase::Done, 87);
    if let Some(t) = &mut s.transfer {
        t.filter = Some(MatchStatus::Uncertain);
    }
    s
}

pub fn transfers_empty() -> UiState {
    let mut s = yt_connected(Screen::Transfers);
    s.resumable.insert(
        "pl0000000000000000000B".into(),
        ResumeInfo { next_index: 31 },
    );
    s
}
