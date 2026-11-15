//! Setup: app credentials (client IDs) and the redirect URI to register.

use egui::{RichText, TextEdit, Ui};

use crate::core::config::{
    self, ENV_GOOGLE_CLIENT_ID, ENV_GOOGLE_CLIENT_SECRET, ENV_SPOTIFY_CLIENT_ID,
};
use crate::core::error::README_SETUP_URL;
use crate::gui::state::{Screen, UiState};
use crate::gui::widgets::{self, card, centered, muted};
use crate::gui::worker::Command;

fn field(
    ui: &mut Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    env: Option<&str>,
    password: bool,
) {
    ui.label(RichText::new(label).strong());
    let edit = TextEdit::singleline(value)
        .hint_text(hint)
        .password(password)
        .font(egui::TextStyle::Monospace)
        .desired_width(f32::INFINITY);
    ui.add_enabled(env.is_none(), edit);
    if let Some(var) = env {
        ui.label(
            RichText::new(format!("Set by the {var} environment variable."))
                .color(muted(ui))
                .small(),
        );
    }
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        centered(ui, 760.0, |ui| {
            ui.add_space(8.0);
            widgets::heading(ui, "Set up your app credentials");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label("playlist-porter uses your own Spotify and Google developer apps. Create them first (see the ");
                ui.hyperlink_to("README setup steps", README_SETUP_URL);
                ui.label("), then paste the IDs here. These are app credentials, not your account sign-ins. They're saved to ");
                let path = config::config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "~/.config/playlist-porter/config.toml".into());
                ui.label(RichText::new(path).monospace());
                ui.label(".");
            });
            ui.add_space(14.0);

            let uri = s.config.redirect_uri();
            card(ui, |ui| {
                widgets::section_title(ui, "Redirect URI to register");
                ui.label(
                    RichText::new(
                        "Add this exact URI to both the Spotify Developer Dashboard and your Google OAuth client. Use the IP address, not \u{201c}localhost\u{201d}.",
                    )
                    .color(muted(ui)),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.scope(|ui| {
                        ui.set_width(ui.available_width() - 96.0);
                        widgets::code_box(ui, &uri, 1);
                    });
                    if ui.add(widgets::button("🗐 Copy")).clicked() {
                        ui.ctx().copy_text(uri.clone());
                    }
                });
            });
            ui.add_space(12.0);

            card(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::section_title(ui, "Spotify");
                    ui.label(RichText::new("Required").color(muted(ui)));
                });
                ui.add_space(4.0);
                let env = s.env.spotify_client_id.then_some(ENV_SPOTIFY_CLIENT_ID);
                field(ui, "Client ID", &mut s.setup.spotify_client_id, "32-character client ID", env, false);
                ui.label(
                    RichText::new(
                        "No client secret needed: sign-in uses PKCE in your browser. Don't put \u{201c}Spotify\u{201d} in your developer app's name.",
                    )
                    .color(muted(ui)),
                );
            });
            ui.add_space(12.0);

            card(ui, |ui| {
                ui.horizontal(|ui| {
                    widgets::section_title(ui, "Google / YouTube");
                    ui.label(RichText::new("Only needed for \u{201c}Send to YouTube\u{201d}").color(muted(ui)));
                });
                ui.add_space(4.0);
                ui.columns(2, |cols| {
                    let env = s.env.google_client_id.then_some(ENV_GOOGLE_CLIENT_ID);
                    field(&mut cols[0], "Client ID", &mut s.setup.google_client_id, "…apps.googleusercontent.com", env, false);
                    let env = s.env.google_client_secret.then_some(ENV_GOOGLE_CLIENT_SECRET);
                    field(&mut cols[1], "Client secret", &mut s.setup.google_client_secret, "GOCSPX-…", env, true);
                });
                ui.label(
                    RichText::new(
                        "Create a \u{201c}Desktop app\u{201d} OAuth client in a Google Cloud project with YouTube Data API v3 enabled. Keep the consent screen in testing mode and add yourself as a test user.",
                    )
                    .color(muted(ui)),
                );
            });

            if let Some(e) = &s.setup.error {
                ui.add_space(8.0);
                ui.colored_label(widgets::err_color(ui), format!("⚠ {e}"));
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Environment variables (PLAYLIST_PORTER_*) override these values.")
                        .color(muted(ui)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = ui.add_enabled(!s.setup.saving, widgets::primary_button(ui, "Save and continue"));
                    if s.setup.from_settings && ui.add(widgets::button("Cancel")).clicked() {
                        s.setup.from_settings = false;
                        s.screen = Screen::Settings;
                    }
                    if save.clicked() {
                        submit(s, out);
                    }
                });
            });
            ui.add_space(16.0);
        });
    });
}

fn submit(s: &mut UiState, out: &mut Vec<Command>) {
    let spotify = s.setup.spotify_client_id.trim().to_string();
    let gid = s.setup.google_client_id.trim().to_string();
    let gsecret = s.setup.google_client_secret.trim().to_string();
    if spotify.is_empty() && !s.env.spotify_client_id {
        s.setup.error = Some("Enter your Spotify client ID.".into());
        return;
    }
    if !spotify.is_empty() && !spotify.chars().all(|c| c.is_ascii_alphanumeric()) {
        s.setup.error =
            Some("The Spotify client ID should contain only letters and digits.".into());
        return;
    }
    if gid.is_empty() != gsecret.is_empty()
        && !(s.env.google_client_id || s.env.google_client_secret)
    {
        s.setup.error =
            Some("Enter both the Google client ID and secret, or leave both empty.".into());
        return;
    }
    let mut cfg = s.file_config.clone();
    cfg.spotify_client_id = spotify;
    cfg.google_client_id = gid;
    cfg.google_client_secret = gsecret;
    s.setup.error = None;
    s.setup.saving = true;
    out.push(Command::SaveConfig(Box::new(cfg)));
}
