//! One module per screen. Each `show` draws from `UiState` and pushes
//! commands; none of them do I/O.

pub mod about;
pub mod detail;
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
    let _ = (ui, s, out);
}

/// Button handlers shared by the playlist table and the detail screen.
pub mod actions {
    use crate::gui::state::{PlaylistRow, UiState};
    use crate::gui::worker::Command;

    pub fn start_export(s: &mut UiState, out: &mut Vec<Command>, p: &PlaylistRow) {
        let _ = (s, out, p);
    }

    pub fn start_youtube(s: &mut UiState, out: &mut Vec<Command>, p: &PlaylistRow, resume: bool) {
        let _ = (s, out, p, resume);
    }
}
