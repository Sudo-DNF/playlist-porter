//! UI state: plain data, no business logic and no I/O. Screens read and
//! mutate it and push [`Command`](super::worker::Command)s for the worker.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::core::config::{Config, EnvOverrides, ExportFormat};
use crate::core::error::CoreError;
use crate::core::spotify::models::{Entry, SkipCounts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Waiting for the worker to load config and check the keyring.
    #[default]
    Starting,
    Setup,
    SignIn,
    Playlists,
    Detail,
    Transfers,
    Settings,
    About,
}

impl Screen {
    /// Screens that show Spotify data and therefore need the attribution footer.
    pub fn shows_spotify_data(self) -> bool {
        matches!(self, Screen::Playlists | Screen::Detail | Screen::Transfers)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Spotify,
    YouTube,
}

impl Service {
    pub fn label(self) -> &'static str {
        match self {
            Service::Spotify => "Spotify",
            Service::YouTube => "YouTube",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Conn {
    #[default]
    Unknown,
    Disconnected,
    Connected {
        name: String,
        id: String,
    },
}

impl Conn {
    pub fn is_connected(&self) -> bool {
        matches!(self, Conn::Connected { .. })
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Conn::Connected { name, .. } => Some(name),
            _ => None,
        }
    }
}

/// An error shown in an error panel or toast, with an actionable hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiError {
    pub message: String,
    pub hint: Option<String>,
    pub setup_link: bool,
}

impl From<&CoreError> for UiError {
    fn from(e: &CoreError) -> Self {
        UiError {
            message: e.to_string(),
            hint: e.hint().map(str::to_owned),
            setup_link: e.wants_setup_link(),
        }
    }
}

impl From<CoreError> for UiError {
    fn from(e: CoreError) -> Self {
        UiError::from(&e)
    }
}

/// Browser sign-in progress for one service.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SignIn {
    #[default]
    Idle,
    /// Preparing the listener / opening the browser.
    Starting,
    Waiting {
        auth_url: String,
        port: u16,
        deadline: Instant,
    },
    /// Code received; exchanging it and loading the profile.
    Finishing,
}

impl SignIn {
    pub fn is_busy(&self) -> bool {
        !matches!(self, SignIn::Idle)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SetupForm {
    pub spotify_client_id: String,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub error: Option<String>,
    /// Opened from Settings → Edit setup… (shows a Cancel button).
    pub from_settings: bool,
    pub saving: bool,
}

/// One row of the playlist table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRow {
    pub id: String,
    pub name: String,
    pub owner_name: String,
    pub owner_is_me: bool,
    pub collaborative: bool,
    /// `None` when Spotify doesn't return `items` (followed playlists).
    pub total: Option<u32>,
    pub spotify_url: String,
    pub exportable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ListState {
    pub playlists: Vec<PlaylistRow>,
    pub loading: bool,
    pub loaded_at: Option<Instant>,
    pub filter: String,
    pub only_exportable: bool,
    pub error: Option<UiError>,
}

impl ListState {
    /// Rows matching the filter box and the exportable toggle. Filters only
    /// the already loaded list; never searches Spotify.
    pub fn visible(&self) -> Vec<&PlaylistRow> {
        let needle = self.filter.trim().to_lowercase();
        self.playlists
            .iter()
            .filter(|p| !self.only_exportable || p.exportable)
            .filter(|p| {
                needle.is_empty()
                    || p.name.to_lowercase().contains(&needle)
                    || p.owner_name.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn find(&self, id: &str) -> Option<&PlaylistRow> {
        self.playlists.iter().find(|p| p.id == id)
    }
}

#[derive(Debug, Clone)]
pub struct DetailState {
    pub row: PlaylistRow,
    pub entries: Option<Vec<Entry>>,
    pub error: Option<UiError>,
}

impl DetailState {
    pub fn skipped(&self) -> SkipCounts {
        self.entries
            .as_deref()
            .map(SkipCounts::of)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportStep {
    /// Fetching the playlist's tracks.
    Loading,
    Form,
    /// The chosen file exists; ask before replacing it.
    ConfirmOverwrite,
    Writing,
}

#[derive(Debug, Clone)]
pub struct ExportDialog {
    pub playlist_id: String,
    pub playlist_name: String,
    pub format: ExportFormat,
    pub path: String,
    pub step: ExportStep,
    pub track_count: usize,
    pub skipped: SkipCounts,
    pub error: Option<UiError>,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    /// Folder to open with "Show in folder".
    pub folder: Option<PathBuf>,
    pub until: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
}

/// What a disconnect actually deleted, listed for the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisconnectReport {
    pub title: String,
    pub done: Vec<String>,
    pub problems: Vec<String>,
    pub spotify_disconnected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub screen: Screen,
    pub config: Config,
    pub file_config: Config,
    pub env: EnvOverrides,
    pub setup: SetupForm,

    pub spotify: Conn,
    pub youtube: Conn,
    pub spotify_signin: SignIn,
    pub google_signin: SignIn,
    /// Error panel on the sign-in screen (e.g. Spotify 403).
    pub signin_error: Option<UiError>,
    /// Keyring problem (no Secret Service, etc.), shown prominently.
    pub keyring_error: Option<UiError>,

    pub list: ListState,
    pub detail: Option<DetailState>,
    pub export: Option<ExportDialog>,
    pub transfer: Option<super::transfer_state::TransferView>,
    /// Spotify playlist ID → saved resume state summary.
    pub resumable: HashMap<String, ResumeInfo>,

    pub settings_port: String,
    pub settings_port_error: Option<String>,
    pub disconnect_report: Option<DisconnectReport>,
    pub disconnecting: bool,

    pub rate_limited_until: Option<Instant>,
    pub toasts: Vec<Toast>,
    /// General error panel (top of the current screen).
    pub error: Option<UiError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeInfo {
    pub next_index: usize,
}

impl UiState {
    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>, folder: Option<PathBuf>) {
        self.toasts.push(Toast {
            text: text.into(),
            kind,
            folder,
            until: Instant::now() + Duration::from_secs(8),
        });
    }

    /// Clear everything that came from Spotify.
    pub fn clear_spotify_data(&mut self) {
        self.list = ListState {
            filter: std::mem::take(&mut self.list.filter),
            only_exportable: self.list.only_exportable,
            ..Default::default()
        };
        self.detail = None;
        self.export = None;
        self.transfer = None;
        self.resumable.clear();
    }

    pub fn me_id(&self) -> Option<&str> {
        match &self.spotify {
            Conn::Connected { id, .. } => Some(id),
            _ => None,
        }
    }
}

/// "loaded 2 min ago"
pub fn ago(since: Instant, now: Instant) -> String {
    let s = now.saturating_duration_since(since).as_secs();
    match s {
        0..=59 => "loaded just now".into(),
        60..=3599 => format!("loaded {} min ago", s / 60),
        _ => format!("loaded {} h ago", s / 3600),
    }
}

/// "4:32"
pub fn mmss(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn fmt_duration_ms(ms: u64) -> String {
    let s = (ms + 500) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, exportable: bool) -> PlaylistRow {
        PlaylistRow {
            id: name.into(),
            name: name.into(),
            owner_name: "me".into(),
            owner_is_me: exportable,
            collaborative: false,
            total: Some(1),
            spotify_url: String::new(),
            exportable,
        }
    }

    #[test]
    fn filter_and_toggle() {
        let mut l = ListState {
            playlists: vec![row("Road Trip", true), row("Gym", false)],
            ..Default::default()
        };
        assert_eq!(l.visible().len(), 2);
        l.only_exportable = true;
        assert_eq!(l.visible().len(), 1);
        l.only_exportable = false;
        l.filter = "  ROAD ".into();
        assert_eq!(l.visible()[0].name, "Road Trip");
        l.filter = "nothing".into();
        assert!(l.visible().is_empty());
    }

    #[test]
    fn formatting() {
        assert_eq!(mmss(Duration::from_secs(272)), "4:32");
        assert_eq!(fmt_duration_ms(257_400), "4:17");
        assert_eq!(fmt_duration_ms(3_600_000), "1:00:00");
        let now = Instant::now();
        assert_eq!(ago(now, now + Duration::from_secs(130)), "loaded 2 min ago");
    }
}
