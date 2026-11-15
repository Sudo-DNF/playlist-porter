//! UI state for the YouTube transfer flow:
//! notice → Google sign-in → confirmation → progress → report.

use crate::core::matching::MatchStatus;
use crate::core::spotify::models::SkipCounts;
use crate::core::youtube::report::{Counts, ReportRow};

use super::state::UiError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferPhase {
    /// Step 1: YouTube ToS / privacy notice before the first Google sign-in.
    Notice,
    /// Step 2: waiting for Google sign-in in the browser.
    SigningIn,
    /// Fetching the playlist from Spotify for the confirmation.
    Preparing,
    /// Step 3: the unskippable confirmation modal.
    Confirm,
    Running,
    QuotaExhausted,
    Cancelled,
    Done,
    Failed(UiError),
}

/// Numbers for the confirmation modal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfirmInfo {
    /// Tracks the run will process (all, or the remaining ones on resume).
    pub tracks: usize,
    pub total_tracks: usize,
    pub skipped: SkipCounts,
    pub units: u32,
    pub days: u32,
    /// "3:00 AM" local.
    pub reset_local: String,
    /// Resuming into an existing YouTube playlist.
    pub resuming: bool,
}

#[derive(Debug, Clone)]
pub struct TransferView {
    pub playlist_id: String,
    pub playlist_name: String,
    pub playlist_url: String,
    pub resume: bool,
    pub phase: TransferPhase,
    pub confirm: Option<ConfirmInfo>,
    pub total: usize,
    pub done: usize,
    pub current: Option<String>,
    pub rows: Vec<ReportRow>,
    pub filter: Option<MatchStatus>,
    pub youtube_playlist_id: Option<String>,
    pub cancel_requested: bool,
}

impl TransferView {
    pub fn new(id: &str, name: &str, url: &str, resume: bool, phase: TransferPhase) -> Self {
        TransferView {
            playlist_id: id.into(),
            playlist_name: name.into(),
            playlist_url: url.into(),
            resume,
            phase,
            confirm: None,
            total: 0,
            done: 0,
            current: None,
            rows: Vec::new(),
            filter: None,
            youtube_playlist_id: None,
            cancel_requested: false,
        }
    }

    /// A run is in progress (or about to start): block other transfers.
    pub fn is_running(&self) -> bool {
        matches!(self.phase, TransferPhase::Running)
    }

    pub fn is_finished(&self) -> bool {
        matches!(
            self.phase,
            TransferPhase::QuotaExhausted
                | TransferPhase::Cancelled
                | TransferPhase::Done
                | TransferPhase::Failed(_)
        )
    }

    pub fn counts(&self) -> Counts {
        Counts::of(&self.rows)
    }

    pub fn visible_rows(&self) -> Vec<&ReportRow> {
        self.rows
            .iter()
            .filter(|r| self.filter.is_none_or(|f| r.status == f))
            .collect()
    }
}
