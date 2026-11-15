//! Sign in: connect Spotify in the system browser.

use std::time::Instant;

use egui::{RichText, Ui};

use crate::gui::state::{mmss, Service, SignIn, UiState};
use crate::gui::widgets::{self, card, centered, muted};
use crate::gui::worker::Command;

/// The waiting card shared by the Spotify and Google sign-in flows: spinner,
/// countdown, Cancel, and the auth URL as selectable text.
pub fn waiting_card(ui: &mut Ui, state: &SignIn, service: Service, out: &mut Vec<Command>) {
    card(ui, |ui| match state {
        SignIn::Waiting {
            auth_url,
            port,
            deadline,
        } => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(26.0));
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("Waiting for you to approve access in your browser…")
                            .strong(),
                    );
                    let left = deadline.saturating_duration_since(Instant::now());
                    ui.label(
                        RichText::new(format!(
                            "Listening on 127.0.0.1:{port} · times out in {}",
                            mmss(left)
                        ))
                        .color(muted(ui)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(widgets::button("Cancel")).clicked() {
                        out.push(Command::CancelSignIn(service));
                    }
                });
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
            ui.add_space(10.0);
            ui.label(
                RichText::new("Browser didn't open? Copy this address into it:").color(muted(ui)),
            );
            widgets::code_box(ui, auth_url, 3);
        }
        _ => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(if matches!(state, SignIn::Finishing) {
                    "Finishing sign-in…"
                } else {
                    "Opening your browser…"
                });
            });
        }
    });
}

pub fn show(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        centered(ui, 760.0, |ui| {
            ui.add_space(36.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Connect your Spotify account").size(26.0).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "playlist-porter reads the playlists you own or collaborate on. It asks only for permission to read your private and collaborative playlists.",
                    )
                    .color(muted(ui)),
                );
            });
            ui.add_space(20.0);

            if let Some(e) = s.signin_error.clone() {
                if widgets::error_panel(ui, &e) {
                    s.signin_error = None;
                }
                ui.add_space(12.0);
            }

            if s.spotify_signin.is_busy() {
                waiting_card(ui, &s.spotify_signin, Service::Spotify, out);
            } else {
                card(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(6.0);
                        let ready = s.config.has_spotify();
                        let btn = ui.add_enabled(
                            ready,
                            widgets::primary_button(ui, "Connect Spotify ⤴").min_size(egui::vec2(200.0, 36.0)),
                        );
                        if btn.clicked() {
                            s.signin_error = None;
                            s.spotify_signin = SignIn::Starting;
                            out.push(Command::SignIn(Service::Spotify));
                        }
                        ui.add_space(4.0);
                        ui.label(RichText::new("Spotify sign-in opens in your system browser.").color(muted(ui)));
                        ui.add_space(6.0);
                    });
                });
            }

            ui.add_space(16.0);
            card(ui, |ui| {
                widgets::section_title(ui, "About Spotify Development Mode");
                ui.add_space(4.0);
                ui.label("• Up to 5 people can use this app. The app owner adds each one under User Management in the Spotify Developer Dashboard.");
                ui.label("• The app owner needs an active Spotify Premium subscription. If it lapses, sign-in stops working for everyone.");
            });
            ui.add_space(16.0);
        });
    });
}
