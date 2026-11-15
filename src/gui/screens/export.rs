//! Export dialog: format, save location, overwrite confirmation.

use std::path::Path;

use egui::{Align, Layout, RichText, TextEdit, Ui};

use crate::core::config::ExportFormat;
use crate::gui::state::{ExportStep, UiState};
use crate::gui::widgets::{self, muted};
use crate::gui::worker::Command;

fn with_extension(path: &str, format: ExportFormat) -> String {
    let p = Path::new(path);
    match p.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("csv") || e.eq_ignore_ascii_case("json") => {
            p.with_extension(format.extension()).display().to_string()
        }
        _ => path.to_string(),
    }
}

pub fn modal(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(d) = &mut s.export else { return };
    let mut close = false;
    let ctx = ui.ctx().clone();
    let resp = egui::Modal::new(egui::Id::unique("export_modal")).show(&ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 80.0).clamp(320.0, 600.0));
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Export \u{201c}{}\u{201d}", d.playlist_name))
                    .size(18.0)
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("✖").on_hover_text("Close").clicked() {
                    close = true;
                }
            });
        });
        ui.separator();

        match d.step {
            ExportStep::Loading => {
                ui.add_space(8.0);
                if let Some(e) = &d.error {
                    widgets::error_panel(ui, e);
                } else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading tracks from Spotify…");
                    });
                }
                ui.add_space(8.0);
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(widgets::button("Cancel")).clicked() {
                        close = true;
                    }
                });
            }
            ExportStep::ConfirmOverwrite => {
                ui.add_space(8.0);
                let p = Path::new(&d.path);
                let file = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let dir = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
                widgets::callout(ui, widgets::warn_color(ui), |ui| {
                    ui.label(
                        RichText::new(format!("⚠ \u{201c}{file}\u{201d} already exists in {dir}."))
                            .strong(),
                    );
                    ui.label("Replace it? The existing file will be overwritten.");
                });
                ui.add_space(8.0);
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(widgets::danger_button("Replace")).clicked() {
                        d.step = ExportStep::Writing;
                        out.push(Command::WriteExport {
                            id: d.playlist_id.clone(),
                            path: d.path.clone(),
                            format: d.format,
                            overwrite: true,
                        });
                    }
                    if ui.add(widgets::button("Back")).clicked() {
                        d.step = ExportStep::Form;
                    }
                });
            }
            ExportStep::Form | ExportStep::Writing => {
                let busy = d.step == ExportStep::Writing;
                ui.add_space(4.0);
                ui.label(RichText::new("Format").strong());
                let before = d.format;
                ui.radio_value(&mut d.format, ExportFormat::Csv, "CSV");
                ui.indent("csv_desc", |ui| {
                    ui.label(
                        RichText::new("Opens in any spreadsheet app. First line credits Spotify and links the playlist.")
                            .color(muted(ui)),
                    );
                });
                ui.radio_value(&mut d.format, ExportFormat::Json, "JSON");
                ui.indent("json_desc", |ui| {
                    ui.label(
                        RichText::new("Includes attribution and playlist_url fields at the top level.")
                            .color(muted(ui)),
                    );
                });
                if d.format != before {
                    d.path = with_extension(&d.path, d.format);
                }
                ui.add_space(8.0);
                let save_label = ui.label(RichText::new("Save to").strong());
                ui.horizontal(|ui| {
                    let w = ui.available_width() - widgets::button_width(ui, "Choose…") - 8.0;
                    ui.add(
                        TextEdit::singleline(&mut d.path)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(w),
                    )
                    .labelled_by(save_label.id);
                    if ui.add_enabled(!busy, widgets::button("Choose…")).clicked() {
                        out.push(Command::ChooseExportPath {
                            current: d.path.clone(),
                            format: d.format,
                        });
                    }
                });
                ui.add_space(8.0);
                widgets::callout(ui, muted(ui), |ui| {
                    let mut text = format!(
                        "{} track{} will be exported, each with its Spotify link.",
                        d.track_count,
                        if d.track_count == 1 { "" } else { "s" }
                    );
                    if d.skipped.total() > 0 {
                        text.push_str(&format!(
                            " {} item{} skipped ({}).",
                            d.skipped.total(),
                            if d.skipped.total() == 1 { " is" } else { "s are" },
                            d.skipped.describe()
                        ));
                    }
                    ui.label(text);
                });
                if let Some(e) = &d.error {
                    ui.add_space(6.0);
                    ui.colored_label(widgets::err_color(ui), format!("⚠ {}", e.message));
                }
                ui.add_space(8.0);
                ui.separator();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ok = !busy && !d.path.trim().is_empty() && d.track_count > 0;
                    let label = if busy { "Exporting…" } else { "Export" };
                    let b = ui.add_enabled(ok, widgets::primary_button(ui, label));
                    let b = if d.track_count == 0 {
                        b.on_disabled_hover_text("There are no tracks to export.")
                    } else {
                        b
                    };
                    if b.clicked() {
                        d.error = None;
                        d.step = ExportStep::Writing;
                        out.push(Command::WriteExport {
                            id: d.playlist_id.clone(),
                            path: d.path.clone(),
                            format: d.format,
                            overwrite: false,
                        });
                    }
                    if ui.add(widgets::button("Cancel")).clicked() {
                        close = true;
                    }
                });
            }
        }
    });
    if (resp.should_close() && d.step != ExportStep::Writing) || close {
        s.export = None;
        out.push(Command::CloseExport);
    }
}
