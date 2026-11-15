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
}

/// Button handlers shared by the playlist table and the detail screen.
pub mod actions {
    use crate::gui::state::{ExportDialog, ExportStep, PlaylistRow, UiState};
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

    pub fn start_youtube(s: &mut UiState, out: &mut Vec<Command>, p: &PlaylistRow, resume: bool) {
        let _ = (s, out, p, resume);
    }
}
