//! About / Privacy: disclaimer, what's stored and where, revoke links,
//! YouTube ToS and Google Privacy Policy, version.

use egui::{RichText, Ui};

use super::settings::SPOTIFY_APPS;
use super::transfers::{youtube_terms_text, GOOGLE_PERMISSIONS, GOOGLE_PRIVACY, YOUTUBE_TOS};
use crate::core::config;
use crate::core::error::README_SETUP_URL;
use crate::core::youtube::resume;
use crate::core::DISCLAIMER;
use crate::gui::state::UiState;
use crate::gui::widgets::{self, card, centered, muted};
use crate::gui::worker::Command;

pub const README_URL: &str = "https://github.com/Sudo-DNF/playlist-porter#readme";

fn bullet_link(ui: &mut Ui, text: &str, url: &str) {
    ui.horizontal(|ui| {
        ui.label("•");
        ui.hyperlink_to(text, url);
    });
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let _ = (s, out);
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        centered(ui, 900.0, |ui| {
            ui.horizontal(|ui| {
                widgets::brand_mark(ui, 40.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("playlist-porter").size(24.0).strong());
                    ui.label(
                        RichText::new(format!(
                            "Version {} · Personal, non-commercial use",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .color(muted(ui)),
                    );
                });
            });
            ui.add_space(12.0);

            card(ui, |ui| {
                widgets::section_title(ui, "Disclaimer");
                ui.label(DISCLAIMER);
            });
            ui.add_space(12.0);

            card(ui, |ui| {
                widgets::section_title(ui, "What\u{2019}s stored, and where");
                ui.add_space(4.0);
                let cfg = config::config_path()
                    .map(|p| widgets::tilde(&p))
                    .unwrap_or_else(|| "~/.config/playlist-porter/config.toml".into());
                let res = resume::default_dir()
                    .map(|p| format!("{}/<playlist-id>.json", widgets::tilde(&p)))
                    .unwrap_or_else(|| "~/.local/state/playlist-porter/resume/<playlist-id>.json".into());
                let rows: [(&str, String, &str); 5] = [
                    (
                        "Spotify and Google sign-in tokens",
                        "OS keyring, service playlist-porter, entries spotify and google".into(),
                        "Settings → Disconnect or Sign out of everything",
                    ),
                    (
                        "App credentials and UI preferences",
                        cfg,
                        "Delete the file. It holds no account data.",
                    ),
                    (
                        "Paused YouTube transfer",
                        format!("{res}. Video IDs and progress only, no track details."),
                        "Automatically when the transfer finishes, after 7 days, or on sign-out",
                    ),
                    (
                        "Your playlists and tracks",
                        "Memory only, while the app is open".into(),
                        "Cleared on sign-out and when you quit",
                    ),
                    (
                        "Export files you save",
                        "Wherever you chose".into(),
                        "They\u{2019}re yours. The app doesn\u{2019}t track or delete them.",
                    ),
                ];
                let w = ui.available_width();
                let col = [
                    (w * 0.27).max(110.0),
                    (w * 0.38).max(150.0),
                    (w * 0.35 - 36.0).max(130.0),
                ];
                egui::Grid::new("stored")
                    .striped(true)
                    .num_columns(3)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for h in ["What", "Where", "How it\u{2019}s removed"] {
                            ui.label(RichText::new(h).color(muted(ui)));
                        }
                        ui.end_row();
                        for (what, where_, how) in rows {
                            widgets::text_cell(ui, col[0], what);
                            widgets::text_cell(ui, col[1], where_);
                            widgets::text_cell(ui, col[2], how);
                            ui.end_row();
                        }
                    });
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Never stored: audio, cover art, usage data. The app sends no analytics, crash reports or update checks, and talks only to Spotify\u{2019}s and Google\u{2019}s official API servers.")
                        .color(muted(ui)),
                );
            });
            ui.add_space(12.0);

            let youtube = |ui: &mut Ui| {
                card(ui, |ui| {
                    widgets::section_title(ui, "YouTube");
                    youtube_terms_text(ui);
                    ui.label("It searches for videos and creates private playlists on your account. It never plays or downloads anything.");
                    bullet_link(ui, "YouTube Terms of Service", YOUTUBE_TOS);
                    bullet_link(ui, "Google Privacy Policy", GOOGLE_PRIVACY);
                    bullet_link(ui, "Revoke access in your Google Account", GOOGLE_PERMISSIONS);
                });
            };
            let spotify = |ui: &mut Ui| {
                card(ui, |ui| {
                    widgets::section_title(ui, "Spotify");
                    ui.label("Reads only playlists you own or collaborate on, and your Liked Songs. Spotify has no way for apps to revoke access, so remove it yourself after disconnecting.");
                    bullet_link(ui, "Remove access at spotify.com/account/apps", SPOTIFY_APPS);
                    bullet_link(ui, "README: setup and limitations", README_SETUP_URL);
                    bullet_link(ui, "README", README_URL);
                });
            };
            if ui.available_width() > 700.0 {
                ui.columns(2, |c| {
                    youtube(&mut c[0]);
                    spotify(&mut c[1]);
                });
            } else {
                youtube(ui);
                ui.add_space(8.0);
                spotify(ui);
            }
            ui.add_space(16.0);
        });
    });
}
