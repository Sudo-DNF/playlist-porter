//! Playlist detail: the track table, with skipped items marked.

use egui::{Align, Layout, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use super::actions;
use super::playlists::{status_label, EMPTY_TIP, NO_GOOGLE_TIP};
use crate::core::spotify::models::Entry;
use crate::gui::state::{fmt_duration_ms, Screen, UiState};
use crate::gui::widgets::{self, muted, warn_color};
use crate::gui::worker::Command;

fn skipped_badge(ui: &mut Ui, text: &str) {
    widgets::badge(ui, format!("⊘ {text} · skipped"), warn_color(ui));
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(d) = s.detail.clone() else {
        s.screen = Screen::Playlists;
        return;
    };
    if ui.link("‹ All playlists").clicked() {
        s.screen = Screen::Playlists;
        s.detail = None;
        return;
    }
    ui.add_space(6.0);
    let row = d.row.clone();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(&row.name).size(24.0).strong()).truncate());
            ui.horizontal_wrapped(|ui| {
                let owner = if row.owner_is_me {
                    format!("Owner: {} (you)", row.owner_name)
                } else {
                    format!("Owner: {}", row.owner_name)
                };
                ui.label(RichText::new(owner).color(muted(ui)));
                ui.add_space(8.0);
                if let Some(n) = row.total {
                    ui.label(RichText::new(format!("{n} tracks")).color(muted(ui)));
                    ui.add_space(8.0);
                }
                if row.collaborative {
                    widgets::badge(ui, "👥 Collaborative", muted(ui));
                    ui.add_space(8.0);
                }
                status_label(ui, row.exportable);
                ui.add_space(8.0);
                widgets::ext_link(ui, "Open playlist in Spotify", &row.spotify_url);
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            let has_google = s.config.has_google();
            let empty = d
                .entries
                .as_ref()
                .is_some_and(|e| e.iter().all(|x| x.track().is_none()));
            let resumable = s.resumable.contains_key(&row.id);
            let running = s.transfer.as_ref().is_some_and(|t| t.is_running());
            let label = if resumable {
                "→ Resume transfer"
            } else {
                "→ Send to YouTube…"
            };
            let yt = ui.add_enabled(
                has_google && !empty && !running && d.entries.is_some(),
                widgets::primary_button(ui, label),
            );
            let yt = if !has_google {
                yt.on_disabled_hover_text(NO_GOOGLE_TIP)
            } else if empty {
                yt.on_disabled_hover_text(EMPTY_TIP)
            } else {
                yt
            };
            if yt.clicked() {
                actions::start_youtube(s, out, &row, resumable);
            }
            if ui
                .add_enabled(d.entries.is_some(), widgets::button("⬇ Export…"))
                .clicked()
            {
                actions::start_export(s, out, &row);
            }
        });
    });
    ui.add_space(10.0);

    if let Some(e) = &d.error {
        if widgets::error_panel(ui, e) {
            if let Some(d) = &mut s.detail {
                d.error = None;
            }
        }
        return;
    }
    let Some(entries) = &d.entries else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading tracks…");
        });
        return;
    };

    let skipped = d.skipped();
    if skipped.total() > 0 {
        widgets::callout(ui, warn_color(ui), |ui| {
            ui.label(
                RichText::new(format!(
                    "⚠ {} item{} will be skipped in exports and transfers: {}.",
                    skipped.total(),
                    if skipped.total() == 1 { "" } else { "s" },
                    skipped.describe()
                ))
                .color(warn_color(ui)),
            );
        });
        ui.add_space(8.0);
    }
    if entries.is_empty() {
        ui.label(RichText::new("This playlist is empty.").color(muted(ui)));
        return;
    }

    widgets::table_style(ui);
    let gap = ui.spacing().item_spacing.x;
    let num_w = 36.0;
    let dur_w = widgets::text_width(ui, "Duration", egui::TextStyle::Body) + 6.0;
    let isrc_w = widgets::text_width(ui, "XXA000000000", egui::TextStyle::Monospace) + 8.0;
    let link_w = widgets::text_width(ui, "Open in Spotify ⤴", egui::TextStyle::Body) + 6.0;
    let flex = (ui.available_width() - num_w - dur_w - isrc_w - link_w - 7.0 * gap).max(300.0);
    let title_w = (flex * 0.42).max(120.0);
    let artists_w = (flex * 0.27).max(80.0);
    let album_w = (flex - title_w - artists_w).max(80.0);
    egui::ScrollArea::horizontal().show(ui, |ui| {
        TableBuilder::new(ui)
            .striped(true)
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(num_w))
            .column(Column::exact(title_w).clip(true))
            .column(Column::exact(artists_w).clip(true))
            .column(Column::exact(album_w).clip(true))
            .column(Column::exact(dur_w))
            .column(Column::exact(isrc_w))
            .column(Column::exact(link_w))
            .header(28.0, |mut h| {
                for t in [
                    "#", "Title", "Artists", "Album", "Duration", "ISRC", "Spotify",
                ] {
                    h.col(|ui| {
                        if t == "#" || t == "Duration" {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.strong(t);
                            });
                        } else {
                            ui.strong(t);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(34.0, entries.len(), |mut r| {
                    let i = r.index();
                    let e = &entries[i];
                    r.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new((i + 1).to_string()).color(muted(ui)));
                        });
                    });
                    match e {
                        Entry::Track(t) => {
                            r.col(|ui| {
                                ui.add(egui::Label::new(&t.title).truncate())
                                    .on_hover_text(&t.title);
                            });
                            r.col(|ui| {
                                ui.add(egui::Label::new(t.artists_joined()).truncate());
                            });
                            r.col(|ui| {
                                ui.add(egui::Label::new(&t.album).truncate());
                            });
                            r.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.monospace(fmt_duration_ms(t.duration_ms));
                                });
                            });
                            r.col(|ui| {
                                ui.monospace(t.isrc.as_deref().unwrap_or("–"));
                            });
                            r.col(|ui| {
                                widgets::ext_link(ui, "Open in Spotify", &t.spotify_url);
                            });
                        }
                        other => {
                            let (title, badge, artist, dur) = match other {
                                Entry::Local { name, duration_ms } => {
                                    (name.clone(), "Local file", "Local file", Some(*duration_ms))
                                }
                                Entry::Episode { name } => {
                                    (name.clone(), "Episode", "Podcast episode", None)
                                }
                                _ => ("Unavailable track".to_string(), "Unavailable", "—", None),
                            };
                            r.col(|ui| {
                                // Badge first so it stays visible in a narrow column.
                                skipped_badge(ui, badge);
                                ui.add(
                                    egui::Label::new(RichText::new(&title).italics()).truncate(),
                                )
                                .on_hover_text(&title);
                            });
                            r.col(|ui| {
                                ui.label(RichText::new(artist).color(muted(ui)));
                            });
                            r.col(|ui| {
                                ui.label(RichText::new("—").color(muted(ui)));
                            });
                            r.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.monospace(
                                        dur.map(fmt_duration_ms).unwrap_or_else(|| "–".into()),
                                    );
                                });
                            });
                            r.col(|ui| {
                                ui.monospace("–");
                            });
                            r.col(|ui| {
                                ui.label(RichText::new("No link").color(muted(ui)));
                            });
                        }
                    }
                });
            });
    });
}
