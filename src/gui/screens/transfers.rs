//! YouTube transfer: notice → Google sign-in → confirmation (modal) →
//! progress with a live match report.

use egui::{Align, Layout, ProgressBar, RichText, Ui};
use egui_extras::{Column, TableBuilder};

use super::actions;
use super::signin::waiting_card;
use crate::core::auth::google;
use crate::core::matching::{fmt_diff, MatchStatus};
use crate::core::youtube::quota::{COST_PLAYLIST_INSERT, DAILY_LIMIT};
use crate::core::youtube::report::youtube_playlist_url;
use crate::core::youtube::transfer::playlist_description;
use crate::gui::state::{Screen, Service, UiState};
use crate::gui::transfer_state::{TransferPhase, TransferView};
use crate::gui::widgets::{self, card, centered, muted, ok_color, warn_color};
use crate::gui::worker::Command;

pub const YOUTUBE_TOS: &str = "https://www.youtube.com/t/terms";
pub const GOOGLE_PRIVACY: &str = "http://www.google.com/policies/privacy";
pub const GOOGLE_PERMISSIONS: &str = "https://security.google.com/settings/security/permissions";

/// 13100 → "13,100"
pub fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn status_label(ui: &mut Ui, st: MatchStatus) {
    let (icon, color) = match st {
        MatchStatus::Matched => ("✔", ok_color(ui)),
        MatchStatus::Uncertain => ("⚠", warn_color(ui)),
        MatchStatus::NotFound => ("⊗", widgets::err_color(ui)),
    };
    ui.label(RichText::new(format!("{icon} {}", st.label())).color(color));
}

/// The YouTube ToS / privacy notice shown before the first Google sign-in
/// (also reused on the About screen).
pub fn youtube_terms_text(ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("This feature uses the YouTube API Services. ");
        ui.label(RichText::new("By using it, you agree to the ").strong());
        let link = ui.visuals().hyperlink_color;
        ui.hyperlink_to(
            RichText::new("YouTube Terms of Service")
                .strong()
                .color(link),
            YOUTUBE_TOS,
        );
        ui.label(". Google handles your data under the ");
        ui.hyperlink_to("Google Privacy Policy", GOOGLE_PRIVACY);
        ui.label(".");
    });
}

fn notice(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>, name: &str) {
    centered(ui, 820.0, |ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!(
                "Send \u{201c}{name}\u{201d} to YouTube · Step 1 of 3"
            ))
            .color(muted(ui)),
        );
        widgets::heading(ui, "Before you connect YouTube");
        ui.add_space(10.0);
        card(ui, youtube_terms_text);
        ui.add_space(10.0);
        let two_cols = ui.available_width() > 640.0;
        let accesses = |ui: &mut Ui| {
            card(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.label(RichText::new("What playlist-porter accesses").strong());
                    ui.label(
                        "• Search results and video durations, to find a match for each track",
                    );
                    ui.label("• Permission to create private playlists and add videos to them");
                    ui.label(
                        "• Nothing else: it never plays, downloads or reads your watch history",
                    );
                });
            });
        };
        let stores = |ui: &mut Ui| {
            card(ui, |ui| {
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(RichText::new("What it stores, and where").strong());
                ui.label("• Your Google sign-in token, in your OS keyring only");
                ui.label("• A small resume file while a transfer is paused (video IDs and progress). Deleted when it finishes, when you sign out, or after 7 days");
                });
            });
        };
        if two_cols {
            ui.columns(2, |c| {
                accesses(&mut c[0]);
                stores(&mut c[1]);
            });
        } else {
            accesses(ui);
            ui.add_space(8.0);
            stores(ui);
        }
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.label(
                RichText::new("You can disconnect YouTube any time in Settings, which revokes the token. You can also remove access in your ")
                    .color(muted(ui)),
            );
            ui.hyperlink_to("Google Account permissions", GOOGLE_PERMISSIONS);
            ui.label(RichText::new(".").color(muted(ui)));
        });
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Google sign-in opens in your system browser.").color(muted(ui)),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(widgets::primary_button(
                        ui,
                        "Agree and sign in with Google ⤴",
                    ))
                    .clicked()
                {
                    if let Some(t) = &mut s.transfer {
                        t.phase = TransferPhase::SigningIn;
                    }
                    s.google_signin = crate::gui::state::SignIn::Starting;
                    out.push(Command::SignIn(Service::YouTube));
                }
                if ui.add(widgets::button("Cancel")).clicked() {
                    s.transfer = None;
                    s.screen = Screen::Playlists;
                }
            });
        });
    });
}

fn signing_in(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>, name: &str) {
    centered(ui, 820.0, |ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!(
                "Send \u{201c}{name}\u{201d} to YouTube · Step 2 of 3"
            ))
            .color(muted(ui)),
        );
        widgets::heading(ui, "Sign in with Google");
        ui.add_space(10.0);
        waiting_card(ui, &s.google_signin, Service::YouTube, out);
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!(
                "playlist-porter asks only for the {} scope, to create private playlists and add videos.",
                google::SCOPE
            ))
            .color(muted(ui)),
        );
    });
}

fn progress_header(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(t) = &mut s.transfer else { return };
    let narrow = ui.available_width() < 1000.0;
    card(ui, |ui| {
        let progress = |ui: &mut Ui, t: &TransferView| {
            ui.horizontal(|ui| {
                ui.label(format!("{} of {} tracks", t.done.min(t.total), t.total));
                if t.total > 0 {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{}%", t.done * 100 / t.total)).color(muted(ui)),
                        );
                    });
                }
            });
            let frac = if t.total == 0 {
                0.0
            } else {
                t.done as f32 / t.total as f32
            };
            ui.add(ProgressBar::new(frac).desired_height(10.0));
            let line = match (&t.phase, &t.current) {
                (TransferPhase::Running, Some(c)) => format!("Now searching: {c}"),
                (TransferPhase::Running, None) => "Creating the private playlist…".into(),
                (TransferPhase::Done, _) => "Finished".into(),
                (TransferPhase::QuotaExhausted, _) => "Paused: today's quota is used up".into(),
                (TransferPhase::Cancelled, _) => "Cancelled".into(),
                (TransferPhase::Failed(_), _) => "Stopped by an error".into(),
                _ => String::new(),
            };
            ui.add(egui::Label::new(RichText::new(line).color(muted(ui))).truncate());
        };
        let quota = |ui: &mut Ui, used: u32, reset: &str| {
            ui.horizontal(|ui| {
                ui.label("YouTube quota used today");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.monospace(format!(
                        "≈ {} / {}",
                        thousands(used),
                        thousands(DAILY_LIMIT)
                    ));
                });
            });
            ui.add(
                ProgressBar::new(used as f32 / DAILY_LIMIT as f32)
                    .desired_height(10.0)
                    .fill(muted(ui)),
            );
            ui.label(
                RichText::new(format!("Resets at midnight Pacific ({reset} your time)"))
                    .color(muted(ui)),
            );
        };
        let reset = s.quota_reset_local.clone();
        let used = s.quota_used;
        if narrow {
            progress(ui, t);
            ui.add_space(8.0);
            quota(ui, used, &reset);
        } else {
            ui.columns(2, |c| {
                progress(&mut c[0], t);
                quota(&mut c[1], used, &reset);
            });
        }
        if t.is_running() {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let label = if t.cancel_requested {
                    "Cancelling…"
                } else {
                    "Cancel"
                };
                if ui
                    .add_enabled(!t.cancel_requested, widgets::button(label))
                    .clicked()
                {
                    t.cancel_requested = true;
                    out.push(Command::CancelTransfer);
                }
                ui.label(
                    RichText::new("Stops after the current request and saves your place")
                        .color(muted(ui)),
                );
            });
        }
    });
}

fn outcome_panel(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(t) = &s.transfer else { return };
    let c = t.counts();
    let summary = format!(
        "{} matched, {} uncertain, {} not found",
        c.matched, c.uncertain, c.not_found
    );
    let progress_line = format!(
        "{} of {} tracks processed · YouTube quota used today ≈ {} / {} · resets at midnight Pacific ({} your time)",
        t.done.min(t.total),
        t.total,
        thousands(s.quota_used),
        thousands(DAILY_LIMIT),
        s.quota_reset_local
    );
    let yt_link = t.youtube_playlist_id.as_deref().map(youtube_playlist_url);
    let reset = s.quota_reset_local.clone();
    let mut back = false;
    let mut resume = false;
    match &t.phase {
        TransferPhase::Done => {
            widgets::callout(ui, ok_color(ui), |ui| {
                ui.label(RichText::new(format!("✔ Done: {summary}.")).strong());
                ui.horizontal_wrapped(|ui| {
                    if let Some(u) = &yt_link {
                        widgets::ext_link(ui, "Open your new private playlist on YouTube", u);
                    }
                });
                ui.label(
                    RichText::new(
                        "Review Uncertain matches on YouTube; Not found tracks weren't added.",
                    )
                    .color(muted(ui)),
                );
                ui.label(RichText::new(&progress_line).color(muted(ui)).small());
            });
        }
        TransferPhase::QuotaExhausted => {
            widgets::callout(ui, warn_color(ui), |ui| {
                ui.label(RichText::new("⚠ Today's YouTube quota is used up").strong());
                ui.label(format!(
                    "Progress so far is saved ({summary}). Come back after the reset at midnight Pacific ({reset} your time) and choose Resume transfer."
                ));
                ui.label(RichText::new(&progress_line).color(muted(ui)).small());
                ui.horizontal(|ui| {
                    if ui.add(widgets::button("Back to playlists")).clicked() {
                        back = true;
                    }
                    if let Some(u) = &yt_link {
                        widgets::ext_link(ui, "Open the playlist on YouTube", u);
                    }
                });
            });
        }
        TransferPhase::Cancelled => {
            widgets::callout(ui, muted(ui), |ui| {
                ui.label(RichText::new("Transfer cancelled").strong());
                ui.label(format!(
                    "Your place is saved ({summary}). Resume it any time within 7 days."
                ));
                ui.label(RichText::new(&progress_line).color(muted(ui)).small());
                ui.horizontal(|ui| {
                    if ui
                        .add(widgets::primary_button(ui, "Resume transfer"))
                        .clicked()
                    {
                        resume = true;
                    }
                    if ui.add(widgets::button("Back to playlists")).clicked() {
                        back = true;
                    }
                });
            });
        }
        TransferPhase::Failed(e) => {
            widgets::error_panel(ui, e);
            ui.horizontal(|ui| {
                if s.resumable.contains_key(&t.playlist_id)
                    && ui
                        .add(widgets::primary_button(ui, "Resume transfer"))
                        .clicked()
                {
                    resume = true;
                }
                if ui.add(widgets::button("Back to playlists")).clicked() {
                    back = true;
                }
            });
        }
        _ => return,
    }
    ui.add_space(8.0);
    if back {
        s.screen = Screen::Playlists;
    }
    if resume {
        let row = s.list.find(&t.playlist_id).cloned();
        if let Some(row) = row {
            actions::start_youtube(s, out, &row, true);
        }
    }
}

fn report(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(t) = &mut s.transfer else { return };
    let c = t.counts();
    ui.horizontal(|ui| {
        widgets::section_title(ui, "Match report");
        ui.add_space(8.0);
        let tabs = [
            (None, format!("All ({})", c.total())),
            (
                Some(MatchStatus::Matched),
                format!("Matched ({})", c.matched),
            ),
            (
                Some(MatchStatus::Uncertain),
                format!("Uncertain ({})", c.uncertain),
            ),
            (
                Some(MatchStatus::NotFound),
                format!("Not found ({})", c.not_found),
            ),
        ];
        for (f, label) in tabs {
            if ui.selectable_label(t.filter == f, label).clicked() {
                t.filter = f;
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(!t.rows.is_empty(), widgets::button("Save report…"))
                .clicked()
            {
                let mut rows = t.rows.clone();
                rows.sort_by_key(|r| r.position);
                out.push(Command::SaveReport {
                    playlist_name: t.playlist_name.clone(),
                    playlist_url: t.playlist_url.clone(),
                    youtube_playlist_id: t.youtube_playlist_id.clone(),
                    rows,
                });
            }
        });
    });
    ui.add_space(6.0);
    let mut rows: Vec<_> = t.visible_rows().into_iter().cloned().collect();
    rows.sort_by_key(|r| r.position);
    if rows.is_empty() {
        ui.label(
            RichText::new(if t.rows.is_empty() {
                "Results appear here as each track is processed."
            } else {
                "No tracks with this status."
            })
            .color(muted(ui)),
        );
        return;
    }
    widgets::table_style(ui);
    let gap = ui.spacing().item_spacing.x;
    let status_w = widgets::text_width(ui, "⚠ Uncertain", egui::TextStyle::Body) + 8.0;
    let num_w = 34.0;
    let diff_w = widgets::text_width(ui, "Duration diff", egui::TextStyle::Body) + 6.0;
    let note_w = 124.0;
    let links_w = widgets::text_width(ui, "YouTube  Spotify", egui::TextStyle::Body) + 16.0;
    let flex = (ui.available_width() - status_w - num_w - diff_w - note_w - links_w - 7.0 * gap)
        .max(220.0);
    let col_w = flex / 2.0;
    let small = |ui: &Ui, t: &str| RichText::new(t.to_owned()).size(12.0).color(muted(ui));
    TableBuilder::new(ui)
        .striped(true)
        .stick_to_bottom(t.is_running())
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(status_w))
        .column(Column::exact(num_w))
        .column(Column::exact(col_w).clip(true))
        .column(Column::exact(col_w).clip(true))
        .column(Column::exact(diff_w))
        .column(Column::exact(note_w).clip(true))
        .column(Column::exact(links_w))
        .header(26.0, |mut h| {
            for (i, title) in [
                "Status",
                "#",
                "Spotify track",
                "YouTube video",
                "Duration diff",
                "Note",
                "Links",
            ]
            .into_iter()
            .enumerate()
            {
                h.col(|ui| {
                    if i == 1 || i == 4 {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(title);
                        });
                    } else {
                        ui.strong(title);
                    }
                });
            }
        })
        .body(|body| {
            body.rows(42.0, rows.len(), |mut row| {
                let r = &rows[row.index()];
                row.col(|ui| status_label(ui, r.status));
                row.col(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(r.position.to_string()).color(muted(ui)));
                    });
                });
                row.col(|ui| {
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(&r.title).truncate());
                        ui.add(egui::Label::new(small(ui, &r.artists)).truncate());
                    });
                });
                row.col(|ui| {
                    ui.vertical(|ui| match (&r.video_title, &r.video_id) {
                        (Some(title), _) if r.status.is_added() => {
                            ui.add(egui::Label::new(title).truncate());
                            ui.add(
                                egui::Label::new(small(ui, r.channel.as_deref().unwrap_or("")))
                                    .truncate(),
                            );
                        }
                        (None, Some(_)) => {
                            ui.label("YouTube video");
                            ui.label(small(ui, "added earlier"));
                        }
                        _ => {
                            ui.label(RichText::new("Not added").color(muted(ui)));
                        }
                    });
                });
                row.col(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.monospace(fmt_diff(r.duration_diff_secs));
                    });
                });
                row.col(|ui| {
                    ui.add(egui::Label::new(r.note.as_deref().unwrap_or("")).truncate());
                });
                row.col(|ui| {
                    if r.status.is_added() {
                        if let Some(u) = r.youtube_url() {
                            ui.hyperlink_to("YouTube", u);
                        }
                    }
                    ui.hyperlink_to("Spotify", &r.spotify_url);
                });
            });
        });
}

fn empty(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    widgets::heading(ui, "Transfers");
    ui.label(
        RichText::new("No transfer is running. Choose Send to YouTube… on one of your playlists to start one.")
            .color(muted(ui)),
    );
    ui.add_space(12.0);
    if s.resumable.is_empty() {
        return;
    }
    widgets::section_title(ui, "Paused transfers");
    ui.add_space(4.0);
    let mut ids: Vec<_> = s.resumable.iter().map(|(k, v)| (k.clone(), *v)).collect();
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, info) in ids {
        let row = s.list.find(&id).cloned();
        card(ui, |ui| {
            ui.horizontal(|ui| match &row {
                Some(r) => {
                    ui.label(RichText::new(&r.name).strong());
                    ui.label(
                        RichText::new(format!("{} tracks done", info.next_index)).color(muted(ui)),
                    );
                    widgets::ext_link(ui, "Open in Spotify", &r.spotify_url);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(widgets::primary_button(ui, "Resume transfer"))
                            .clicked()
                        {
                            actions::start_youtube(s, out, r, true);
                        }
                    });
                }
                None => {
                    ui.label(
                        RichText::new("A playlist that is no longer in your list").color(muted(ui)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(widgets::button("Discard")).clicked() {
                            out.push(Command::DiscardResume(id.clone()));
                        }
                    });
                }
            });
        });
        ui.add_space(6.0);
    }
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(t) = &s.transfer else {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| empty(ui, s, out));
        return;
    };
    let name = t.playlist_name.clone();
    let url = t.playlist_url.clone();
    match &t.phase {
        TransferPhase::Notice => {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| notice(ui, s, out, &name));
        }
        TransferPhase::SigningIn => signing_in(ui, s, out, &name),
        TransferPhase::Preparing | TransferPhase::Confirm => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Preparing the transfer…");
            });
        }
        _ => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Sending \u{201c}{name}\u{201d} to YouTube"))
                            .size(21.0)
                            .strong(),
                    )
                    .truncate(),
                );
                widgets::ext_link(ui, "Open playlist in Spotify", &url);
            });
            ui.add_space(8.0);
            // The full progress card while running; afterwards a compact
            // outcome panel leaves room for the report at 800×600.
            if s.transfer.as_ref().is_some_and(|t| !t.is_finished()) {
                progress_header(ui, s, out);
                ui.add_space(8.0);
            }
            outcome_panel(ui, s, out);
            report(ui, s, out);
        }
    }
}

/// Step 3: the confirmation modal. It can't be skipped: nothing is created
/// on YouTube until the user clicks the button here.
pub fn confirm_modal(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let Some(t) = &mut s.transfer else { return };
    if !matches!(t.phase, TransferPhase::Preparing | TransferPhase::Confirm) {
        return;
    }
    let ctx = ui.ctx().clone();
    let account = s.youtube.name().unwrap_or("YouTube").to_string();
    let mut cancel = false;
    let mut start = false;
    let resp = egui::Modal::new(egui::Id::unique("yt_confirm")).show(&ctx, |ui| {
        ui.set_width((ctx.content_rect().width() - 60.0).clamp(320.0, 640.0));
        ui.label(RichText::new(format!("Step 3 of 3 · Signed in to YouTube as {account}")).color(muted(ui)));
        let Some(c) = t.confirm.clone() else {
            ui.label(RichText::new("Create a private YouTube playlist?").size(18.0).strong());
            ui.separator();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading the playlist from Spotify…");
            });
            ui.separator();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(widgets::button("Cancel")).clicked() {
                    cancel = true;
                }
            });
            return;
        };
        let title = if c.resuming {
            "Resume the transfer to YouTube?"
        } else {
            "Create a private YouTube playlist?"
        };
        ui.label(RichText::new(title).size(18.0).strong());
        ui.separator();
        widgets::callout(ui, ui.visuals().selection.bg_fill, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label("🔒 ");
                if c.resuming {
                    ui.label("Videos are added to the ");
                    ui.label(RichText::new("private").strong());
                    ui.label(" playlist created earlier on your YouTube account.");
                } else {
                    ui.label("A new ");
                    ui.label(RichText::new("private").strong());
                    ui.label(" playlist will be created on your YouTube account. Only you can see it until you change its visibility on YouTube.");
                }
            });
        });
        ui.add_space(6.0);
        let key_w = 120.0;
        widgets::kv_row(ui, key_w, "Playlist name", |ui, _label| {
            ui.label(&t.playlist_name);
        });
        widgets::kv_row(ui, key_w, "Tracks", |ui, _label| {
            let mut text = if c.resuming {
                format!("{} remaining of {}", c.tracks, c.total_tracks)
            } else {
                c.tracks.to_string()
            };
            if c.skipped.total() > 0 {
                text.push_str(&format!(" (skipping {})", c.skipped.describe()));
            }
            ui.label(text);
        });
        widgets::kv_row(ui, key_w, "Estimated quota", |ui, _label| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("≈ {} units", thousands(c.units))).strong());
                let detail = if c.resuming {
                    "(150 per track)".to_string()
                } else {
                    format!("(150 per track + {COST_PLAYLIST_INSERT} to create the playlist)")
                };
                ui.label(RichText::new(detail).color(muted(ui)));
            });
        });
        widgets::kv_row(ui, key_w, "Time needed", |ui, _label| {
            let left_today = DAILY_LIMIT.saturating_sub(s.quota_used);
            if c.days <= 1 {
                ui.label(format!(
                    "Fits in today's YouTube quota (about {} units left today).",
                    thousands(left_today)
                ));
            } else {
                ui.label(format!(
                    "About {} days. YouTube allows {} units a day, so the transfer pauses when today's quota runs out. Resume it after the reset at midnight Pacific ({} your time).",
                    c.days,
                    thousands(DAILY_LIMIT),
                    c.reset_local
                ));
            }
        });
        if !c.resuming {
            widgets::kv_row(ui, key_w, "Description", |ui, _label| {
                ui.label(RichText::new(playlist_description(&t.playlist_url)).monospace());
            });
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new("Matching is automatic and imperfect. Each track is marked Matched, Uncertain or Not found in the report so you can check it.")
                .color(muted(ui)),
        );
        ui.separator();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if c.resuming { "Resume transfer" } else { "Create private playlist" };
            if ui
                .add_enabled(c.tracks > 0, widgets::primary_button(ui, label))
                .clicked()
            {
                start = true;
            }
            if ui.add(widgets::button("Cancel")).clicked() {
                cancel = true;
            }
        });
    });
    if start {
        t.phase = TransferPhase::Running;
        t.rows.clear();
        s.screen = Screen::Transfers;
        out.push(Command::StartTransfer);
    } else if cancel || resp.should_close() {
        s.transfer = None;
        out.push(Command::CloseTransfer);
    }
}
