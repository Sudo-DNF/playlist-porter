//! Playlists: the main screen. Lists the user's playlists (owned,
//! collaborative and followed), with actions for the exportable ones.

use std::time::Instant;

use egui::{Align, Layout, RichText, TextEdit, Ui};
use egui_extras::{Column, TableBuilder};

use super::actions;
use crate::gui::state::{ago, DetailState, PlaylistRow, Screen, UiState};
use crate::gui::widgets::{self, muted, ok_color};
use crate::gui::worker::Command;

pub const NOT_EXPORTABLE_TIP: &str = "You follow this playlist but don't own it or collaborate on it. Spotify only lets apps move playlists you own or collaborate on.";
pub const EMPTY_TIP: &str = "This playlist has no tracks, so there's nothing to send.";
pub const NO_GOOGLE_TIP: &str = "Add a Google client ID and secret first (Settings → Edit setup…).";

/// "✔ Exportable" / "🔒 Not exportable" with tooltip. Icon + text, never colour alone.
pub fn status_label(ui: &mut Ui, exportable: bool) {
    if exportable {
        ui.label(RichText::new("✔ Exportable").color(ok_color(ui)));
    } else {
        ui.label(
            RichText::new("🔒 Not exportable")
                .color(muted(ui))
                .underline(),
        )
        .on_hover_text(NOT_EXPORTABLE_TIP);
    }
}

pub fn open_detail(s: &mut UiState, out: &mut Vec<Command>, row: &PlaylistRow) {
    s.detail = Some(DetailState {
        row: row.clone(),
        entries: None,
        error: None,
    });
    s.screen = Screen::Detail;
    out.push(Command::OpenDetail(row.id.clone()));
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    // Header: title + counts on the left, filter / toggle / refresh on the right.
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            widgets::heading(ui, "Your playlists");
            let n = s.list.playlists.len();
            let ex = s.list.playlists.iter().filter(|p| p.exportable).count();
            let mut sub = format!(
                "{n} playlist{} · {ex} exportable",
                if n == 1 { "" } else { "s" }
            );
            if let Some(at) = s.list.loaded_at {
                sub.push_str(" · ");
                sub.push_str(&ago(at, Instant::now()));
            }
            ui.label(RichText::new(sub).color(muted(ui)));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let refresh = ui.add_enabled(!s.list.loading, widgets::button("⟳ Refresh"));
            if refresh.clicked() {
                out.push(Command::LoadPlaylists);
            }
            ui.checkbox(&mut s.list.only_exportable, "Show only exportable");
            ui.add(
                TextEdit::singleline(&mut s.list.filter)
                    .hint_text("🔍 Filter loaded playlists")
                    .desired_width((ui.available_width() - 20.0).clamp(120.0, 260.0)),
            );
        });
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new("ℹ Liked Songs and playlists you own or collaborate on can be exported. Playlists you only follow are listed but can't be: Spotify doesn't allow moving them.")
            .color(muted(ui)),
    );
    ui.add_space(8.0);

    if let Some(e) = s.list.error.clone() {
        if widgets::error_panel(ui, &e) {
            s.list.error = None;
        }
        ui.add_space(8.0);
    }

    if s.list.loading && s.list.playlists.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading your playlists…");
        });
        return;
    }

    let rows: Vec<PlaylistRow> = s.list.visible().into_iter().cloned().collect();
    if rows.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            let msg = if s.list.playlists.is_empty() {
                "You don't have any playlists yet.".to_string()
            } else if s.list.filter.trim().is_empty() {
                "None of your playlists are exportable.".to_string()
            } else {
                format!(
                    "No playlists match \u{201c}{}\u{201d}.",
                    s.list.filter.trim()
                )
            };
            ui.label(RichText::new(msg).color(muted(ui)));
            if !s.list.filter.is_empty() && ui.link("Clear filter").clicked() {
                s.list.filter.clear();
            }
        });
        return;
    }

    let has_google = s.config.has_google();
    let mut clicked_detail: Option<PlaylistRow> = None;
    // Column widths follow the window (and the font) so everything fits at 800 px.
    widgets::table_style(ui);
    let any_resume = rows
        .iter()
        .any(|p| p.exportable && s.resumable.contains_key(&p.id));
    let gap = ui.spacing().item_spacing.x;
    let body = egui::TextStyle::Body;
    let tracks_w = widgets::text_width(ui, "Tracks", egui::TextStyle::Body) + 8.0;
    let status_w = widgets::text_width(ui, "🔒 Not exportable", body.clone()) + 6.0;
    let link_w = widgets::text_width(ui, "Open in Spotify ⤴", body) + 6.0;
    let export_w = widgets::button_width(ui, "Export…");
    let actions_w = if any_resume {
        export_w
            + widgets::button_width(ui, "Resume transfer")
            + widgets::button_width(ui, "Start over")
            + 2.0 * gap
    } else {
        export_w + widgets::button_width(ui, "Send to YouTube…") + gap
    } + 4.0;
    let flex =
        (ui.available_width() - tracks_w - status_w - link_w - actions_w - 6.0 * gap).max(180.0);
    let name_w = (flex * 0.58).max(92.0);
    let owner_w = (flex - name_w).max(70.0);
    let compact_badge = owner_w < 170.0;
    egui::ScrollArea::horizontal().show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(name_w).clip(true))
            .column(Column::exact(owner_w).clip(true))
            .column(Column::exact(tracks_w))
            .column(Column::exact(status_w))
            .column(Column::exact(link_w))
            .column(Column::exact(actions_w))
            .header(28.0, |mut h| {
                for (i, t) in ["Name", "Owner", "Tracks", "Status", "Spotify", "Actions"].iter().enumerate() {
                    h.col(|ui| {
                        if i == 2 {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.strong(*t);
                            });
                        } else {
                            ui.strong(*t);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(40.0, rows.len(), |mut row| {
                    let p = &rows[row.index()];
                    row.col(|ui| {
                        let text = if p.exportable {
                            RichText::new(&p.name).strong()
                        } else {
                            RichText::new(&p.name).color(muted(ui))
                        };
                        let resp = ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click()));
                        if p.exportable {
                            if resp.on_hover_text(format!("{}\nClick to show tracks", p.name)).clicked() {
                                clicked_detail = Some(p.clone());
                            }
                        } else {
                            resp.on_hover_text(&p.name);
                        }
                    });
                    row.col(|ui| {
                        let owner = if p.owner_is_me {
                            format!("{} (you)", p.owner_name)
                        } else {
                            p.owner_name.clone()
                        };
                        if p.collaborative {
                            // Badge first so it stays visible when the name is truncated.
                            if compact_badge {
                                widgets::badge(ui, "👥", muted(ui)).on_hover_text("Collaborative playlist");
                            }
                        }
                        ui.add(egui::Label::new(owner).truncate());
                        if p.collaborative && !compact_badge {
                            widgets::badge(ui, "👥 Collaborative", muted(ui));
                        }
                    });
                    row.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            match p.total {
                                Some(n) => ui.label(n.to_string()),
                                None => ui.label("—").on_hover_text("Spotify doesn't share the track count of playlists you only follow."),
                            };
                        });
                    });
                    row.col(|ui| status_label(ui, p.exportable));
                    row.col(|ui| {
                        widgets::ext_link(ui, "Open in Spotify", &p.spotify_url);
                    });
                    row.col(|ui| actions_cell(ui, s, out, p, has_google));
                });
            });
    });
    if let Some(p) = clicked_detail {
        open_detail(s, out, &p);
    }
}

fn actions_cell(
    ui: &mut Ui,
    s: &mut UiState,
    out: &mut Vec<Command>,
    p: &PlaylistRow,
    has_google: bool,
) {
    let export = ui.add_enabled(p.exportable, widgets::button("Export…"));
    let export = if p.exportable {
        export
    } else {
        export.on_disabled_hover_text(NOT_EXPORTABLE_TIP)
    };
    if export.clicked() {
        actions::start_export(s, out, p);
    }
    let running = s.transfer.as_ref().is_some_and(|t| t.is_running());
    if p.exportable && s.resumable.contains_key(&p.id) {
        let resume = ui.add_enabled(
            has_google && !running,
            widgets::primary_button(ui, "Resume transfer"),
        );
        if resume.clicked() {
            actions::start_youtube(s, out, p, true);
        }
        if ui
            .add_enabled(!running, widgets::button("Start over"))
            .on_hover_text("Discard the saved progress and start a new transfer.")
            .clicked()
        {
            actions::start_youtube(s, out, p, false);
        }
        return;
    }
    let empty = p.total == Some(0);
    let enabled = p.exportable && !empty && has_google && !running;
    let yt = ui.add_enabled(enabled, widgets::button("Send to YouTube…"));
    let yt = if !p.exportable {
        yt.on_disabled_hover_text(NOT_EXPORTABLE_TIP)
    } else if empty {
        yt.on_disabled_hover_text(EMPTY_TIP)
    } else if !has_google {
        yt.on_disabled_hover_text(NO_GOOGLE_TIP)
    } else if running {
        yt.on_disabled_hover_text("Another transfer is running.")
    } else {
        yt
    };
    if yt.clicked() {
        actions::start_youtube(s, out, p, false);
    }
}
