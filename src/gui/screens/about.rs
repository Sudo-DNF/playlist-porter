//! Placeholder; implemented in a later step.

use egui::Ui;

use crate::gui::state::UiState;
use crate::gui::worker::Command;

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let _ = (s, out);
    ui.label("Coming soon.");
}
