//! One module per screen. Each `show` draws from `UiState` and pushes
//! commands; none of them do I/O.

pub mod about;
pub mod detail;
pub mod export;
pub mod playlists;
pub mod settings;
pub mod setup;
pub mod signin;
pub mod transfers;

use egui::Ui;

use super::state::UiState;
use super::worker::Command;

/// Modal dialogs drawn above the current screen.
pub fn modals(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    export::modal(ui, s, out);
    transfers::confirm_modal(ui, s, out);
}

/// Button handlers shared by the playlist table and the detail screen.
pub mod actions {
    use crate::gui::state::{ExportDialog, ExportStep, PlaylistRow, Screen, UiState};
    use crate::gui::transfer_state::{TransferPhase, TransferView};
    use crate::gui::worker::Command;

    pub fn start_export(s: &mut UiState, out: &mut Vec<Command>, p: &PlaylistRow) {
        if !p.exportable {
            return;
        }
        s.export = Some(ExportDialog {
            playlist_id: p.id.clone(),
            playlist_name: p.name.clone(),
            format: s.config.last_export_format,
            path: String::new(),
            step: ExportStep::Loading,
            track_count: 0,
            skipped: Default::default(),
            error: None,
        });
        out.push(Command::PrepareExport(p.id.clone()));
    }

    /// Begin the YouTube flow for a playlist: the notice (if YouTube isn't
    /// connected yet) or straight to the confirmation.
    pub fn start_youtube(s: &mut UiState, out: &mut Vec<Command>, p: &PlaylistRow, resume: bool) {
        if !p.exportable || s.transfer.as_ref().is_some_and(|t| t.is_running()) {
            return;
        }
        if !resume && s.resumable.remove(&p.id).is_some() {
            // "Start over": forget the saved place; a new playlist is created.
            out.push(Command::DiscardResume(p.id.clone()));
        }
        let connected = s.youtube.is_connected();
        let phase = if connected {
            TransferPhase::Preparing
        } else {
            TransferPhase::Notice
        };
        s.transfer = Some(TransferView::new(
            &p.id,
            &p.name,
            &p.spotify_url,
            resume,
            phase,
        ));
        if connected {
            out.push(Command::PrepareTransfer {
                id: p.id.clone(),
                resume,
            });
        } else {
            s.screen = Screen::Transfers;
        }
    }
}
