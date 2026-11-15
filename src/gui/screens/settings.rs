//! Settings: account status, the three disconnect flows (run immediately),
//! and preferences (redirect port, theme, app credentials).

use egui::{Align, Layout, RichText, TextEdit, Ui};

use super::transfers::GOOGLE_PERMISSIONS;
use crate::core::config::{parse_port, redirect_uri_for, ThemePref, ENV_REDIRECT_PORT};
use crate::gui::state::{Conn, Screen, UiState};
use crate::gui::widgets::{self, card, centered, muted, ok_color};
use crate::gui::worker::{Command, Disconnect};

pub const SPOTIFY_APPS: &str = "https://www.spotify.com/account/apps";

fn account_row(
    ui: &mut Ui,
    title: &str,
    status: &str,
    connected: bool,
    add_button: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong());
            let (icon, color) = if connected {
                ("✔", ok_color(ui))
            } else {
                ("○", muted(ui))
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(icon).color(color));
                ui.label(RichText::new(status).color(muted(ui)));
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), add_button);
    });
}

fn report_panel(ui: &mut Ui, s: &mut UiState) {
    let Some(r) = &s.disconnect_report else {
        return;
    };
    let mut dismiss = false;
    widgets::callout(ui, ok_color(ui), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("✔ {}", r.title)).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("✖").on_hover_text("Dismiss").clicked() {
                    dismiss = true;
                }
            });
        });
        ui.label("What was deleted:");
        for line in &r.done {
            ui.label(format!("• {line}"));
        }
        for line in &r.problems {
            ui.colored_label(widgets::err_color(ui), format!("⚠ {line}"));
        }
        if r.problems.iter().any(|p| p.contains("Google")) {
            widgets::ext_link(ui, "Google Account permissions", GOOGLE_PERMISSIONS);
        }
        if r.spotify_disconnected {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label("Spotify has no way for apps to revoke access, so remove playlist-porter yourself at ");
                ui.hyperlink_to("spotify.com/account/apps", SPOTIFY_APPS);
                ui.label(".");
            });
        }
        ui.label(
            RichText::new(
                "Export files you created are yours. playlist-porter doesn't track or delete them.",
            )
            .color(muted(ui)),
        );
    });
    if dismiss {
        s.disconnect_report = None;
    }
    ui.add_space(12.0);
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        centered(ui, 900.0, |ui| {
            widgets::heading(ui, "Settings");
            ui.add_space(10.0);
            report_panel(ui, s);

            let busy = s.disconnecting;
            let mut disconnect = None;
            card(ui, |ui| {
                widgets::section_title(ui, "Accounts");
                ui.add_space(6.0);
                let (sp_status, sp_conn) = match &s.spotify {
                    Conn::Connected { name, id } => (format!("Connected as {name} (user ID {id})"), true),
                    _ => ("Not connected".to_string(), false),
                };
                account_row(ui, "Spotify", &sp_status, sp_conn, |ui| {
                    if sp_conn {
                        if ui.add_enabled(!busy, widgets::button("Disconnect Spotify")).clicked() {
                            disconnect = Some(Disconnect::Spotify);
                        }
                    } else if ui.add(widgets::button("Connect Spotify")).clicked() {
                        s.screen = Screen::SignIn;
                    }
                });
                ui.separator();
                let paused = s.resumable.len();
                let paused_text = match paused {
                    0 => String::new(),
                    1 => " · 1 paused transfer saved".into(),
                    n => format!(" · {n} paused transfers saved"),
                };
                let (yt_status, yt_conn) = match &s.youtube {
                    Conn::Connected { name, .. } => (format!("Connected as {name}{paused_text}"), true),
                    _ if !s.config.has_google() => (
                        "Not set up: add a Google client ID and secret under App credentials".to_string(),
                        false,
                    ),
                    _ => (format!("Not connected{paused_text}"), false),
                };
                account_row(ui, "YouTube", &yt_status, yt_conn, |ui| {
                    if ui
                        .add_enabled(yt_conn && !busy, widgets::button("Disconnect YouTube"))
                        .on_disabled_hover_text("YouTube isn't connected.")
                        .clicked()
                    {
                        disconnect = Some(Disconnect::YouTube);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let btn_w = widgets::button_width(ui, "Sign out of everything") + 12.0;
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - btn_w).max(200.0));
                        ui.label(RichText::new("Sign out of everything").strong());
                        ui.label(
                            RichText::new("Deletes both tokens from your keyring, revokes Google access, deletes paused-transfer files and clears all loaded data. Happens immediately.")
                                .color(muted(ui)),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if busy { "Signing out…" } else { "Sign out of everything" };
                        if ui.add_enabled(!busy, widgets::danger_button(label)).clicked() {
                            disconnect = Some(Disconnect::Everything);
                        }
                    });
                });
            });
            if let Some(which) = disconnect {
                s.disconnecting = true;
                s.disconnect_report = None;
                out.push(Command::Disconnect(which));
            }
            ui.add_space(12.0);

            card(ui, |ui| {
                widgets::section_title(ui, "Preferences");
                ui.add_space(6.0);
                let key_w = 160.0;
                widgets::kv_row(ui, key_w, "Redirect port", |ui, label| {
                    let env = s.env.redirect_port;
                    ui.horizontal(|ui| {
                        let edit = ui.add_enabled(
                            !env,
                            TextEdit::singleline(&mut s.settings_port).desired_width(80.0),
                        )
                        .labelled_by(label);
                        let changed = s.settings_port.trim() != s.config.redirect_port().to_string();
                        let apply = ui.add_enabled(!env && changed, widgets::button("Apply"));
                        if apply.clicked()
                            || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && changed)
                        {
                            match parse_port(&s.settings_port) {
                                Ok(p) => {
                                    s.settings_port_error = None;
                                    let mut cfg = s.file_config.clone();
                                    cfg.redirect_port = Some(p);
                                    out.push(Command::SaveConfig(Box::new(cfg)));
                                }
                                Err(e) => s.settings_port_error = Some(e),
                            }
                        }
                    });
                    if let Some(e) = &s.settings_port_error {
                        ui.colored_label(widgets::err_color(ui), format!("⚠ {e}"));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("Must match the redirect URI registered with Spotify and Google:")
                                .color(muted(ui)),
                        );
                        ui.monospace(redirect_uri_for(s.config.redirect_port()));
                    });
                    if env {
                        ui.label(
                            RichText::new(format!("Set by the {ENV_REDIRECT_PORT} environment variable."))
                                .color(muted(ui))
                                .small(),
                        );
                    }
                });
                ui.add_space(6.0);
                widgets::kv_row(ui, key_w, "Theme", |ui, _label| {
                    ui.horizontal(|ui| {
                        let before = s.config.theme;
                        ui.radio_value(&mut s.config.theme, ThemePref::System, "Follow system");
                        ui.radio_value(&mut s.config.theme, ThemePref::Light, "Light");
                        ui.radio_value(&mut s.config.theme, ThemePref::Dark, "Dark");
                        if s.config.theme != before {
                            s.file_config.theme = s.config.theme;
                            out.push(Command::SaveConfig(Box::new(s.file_config.clone())));
                        }
                    });
                });
                ui.add_space(6.0);
                widgets::kv_row(ui, key_w, "App credentials", |ui, _label| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("Spotify and Google client IDs from config.toml").color(muted(ui)),
                        );
                        if ui.add(widgets::button("Edit setup…")).clicked() {
                            s.setup.from_settings = true;
                            s.setup.error = None;
                            s.screen = Screen::Setup;
                        }
                    });
                });
            });
            ui.add_space(16.0);
        });
    });
}
