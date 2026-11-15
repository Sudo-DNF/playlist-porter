//! The eframe application shell: drains worker events into [`UiState`],
//! draws the top bar, footer and current screen, and forwards commands.

use std::time::Instant;

use egui::{Align, Layout, RichText, Ui};
use tokio::runtime::Handle;

use super::screens;
use super::state::{Conn, ExportStep, Screen, Service, SignIn, ToastKind, UiError, UiState};
use super::widgets::{self, muted, ok_color};
use super::worker::{self, Command, Event, WorkerHandle};
use crate::core::config::ThemePref;
use crate::core::error::CoreError;

pub const ATTRIBUTION: &str = "Playlist and track data supplied by Spotify.";

pub struct PorterApp {
    pub state: UiState,
    worker: WorkerHandle,
    applied_theme: Option<ThemePref>,
}

impl PorterApp {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: Handle) -> Self {
        widgets::setup_fonts(&cc.egui_ctx);
        let worker = worker::spawn(&rt, cc.egui_ctx.clone());
        worker.send(Command::Startup);
        PorterApp {
            state: UiState::default(),
            worker,
            applied_theme: None,
        }
    }
}

impl eframe::App for PorterApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let mut out = Vec::new();
        while let Ok(ev) = self.worker.events.try_recv() {
            apply_event(&mut self.state, ev, &mut out);
        }
        if self.applied_theme != Some(self.state.config.theme) {
            apply_theme(ui.ctx(), self.state.config.theme);
            self.applied_theme = Some(self.state.config.theme);
        }
        draw(ui, &mut self.state, &mut out);
        for cmd in out {
            self.worker.send(cmd);
        }
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: ThemePref) {
    ctx.set_theme(match theme {
        ThemePref::System => egui::ThemePreference::System,
        ThemePref::Light => egui::ThemePreference::Light,
        ThemePref::Dark => egui::ThemePreference::Dark,
    });
}

/// Fold one worker event into the UI state.
pub fn apply_event(s: &mut UiState, ev: Event, out: &mut Vec<Command>) {
    match ev {
        Event::ConfigLoaded { merged, file, env } => {
            s.setup.spotify_client_id = file.spotify_client_id.clone();
            s.setup.google_client_id = file.google_client_id.clone();
            s.setup.google_client_secret = file.google_client_secret.clone();
            s.settings_port = merged.redirect_port().to_string();
            s.config = *merged;
            s.file_config = *file;
            s.env = env;
            if s.screen == Screen::Starting && !s.config.has_spotify() {
                s.screen = Screen::Setup;
            }
        }
        Event::ConfigSaved => {
            s.setup.saving = false;
            s.setup.error = None;
            if s.screen == Screen::Setup {
                s.screen = if s.setup.from_settings {
                    Screen::Settings
                } else if s.spotify.is_connected() {
                    Screen::Playlists
                } else {
                    Screen::Starting
                };
                s.setup.from_settings = false;
            }
        }
        Event::SignedOut(Service::Spotify, err) => {
            s.spotify = Conn::Disconnected;
            s.spotify_signin = SignIn::Idle;
            s.clear_spotify_data();
            s.signin_error = err.map(UiError::from);
            // A deliberate disconnect stays on Settings to show the report.
            let stay = matches!(s.screen, Screen::Setup | Screen::About)
                || (s.screen == Screen::Settings && s.disconnecting);
            if !stay {
                s.screen = Screen::SignIn;
            }
        }
        Event::SignedOut(Service::YouTube, err) => {
            s.youtube = Conn::Disconnected;
            s.google_signin = SignIn::Idle;
            if let Some(e) = err {
                s.error = Some(e.into());
            }
        }
        Event::SignInWaiting {
            service,
            auth_url,
            port,
        } => {
            let w = SignIn::Waiting {
                auth_url,
                port,
                deadline: Instant::now() + crate::core::auth::loopback::TIMEOUT,
            };
            match service {
                Service::Spotify => s.spotify_signin = w,
                Service::YouTube => s.google_signin = w,
            }
        }
        Event::SignInFinishing(service) => match service {
            Service::Spotify => s.spotify_signin = SignIn::Finishing,
            Service::YouTube => s.google_signin = SignIn::Finishing,
        },
        Event::SignedIn { service, name, id } => match service {
            Service::Spotify => {
                s.spotify = Conn::Connected { name, id };
                s.spotify_signin = SignIn::Idle;
                s.signin_error = None;
                if matches!(s.screen, Screen::Starting | Screen::SignIn | Screen::Setup) {
                    s.screen = Screen::Playlists;
                }
            }
            Service::YouTube => {
                s.youtube = Conn::Connected { name, id };
                s.google_signin = SignIn::Idle;
            }
        },
        Event::SignInFailed(service, e) => {
            let err = (!matches!(e, CoreError::AuthCancelled)).then(|| UiError::from(&e));
            match service {
                Service::Spotify => {
                    s.spotify_signin = SignIn::Idle;
                    s.signin_error = err;
                    if s.screen == Screen::Starting {
                        s.screen = Screen::SignIn;
                    }
                }
                Service::YouTube => {
                    s.google_signin = SignIn::Idle;
                    if let Some(err) = err {
                        s.error = Some(err);
                    }
                }
            }
        }
        Event::KeyringUnavailable(e) => s.keyring_error = Some(e.into()),
        Event::PlaylistsLoading => s.list.loading = true,
        Event::PlaylistsLoaded(rows) => {
            s.list.playlists = rows;
            s.list.loading = false;
            s.list.loaded_at = Some(Instant::now());
            s.list.error = None;
        }
        Event::PlaylistsFailed(e) => {
            s.list.loading = false;
            s.list.error = Some(e.into());
        }
        Event::DetailLoaded { row, entries } => {
            if let Some(d) = &mut s.detail {
                if d.row.id == row.id {
                    d.row = row;
                    d.entries = Some(entries);
                    d.error = None;
                }
            }
        }
        Event::DetailFailed(id, e) => {
            if let Some(d) = &mut s.detail {
                if d.row.id == id {
                    d.error = Some(e.into());
                }
            }
        }
        Event::ExportReady {
            id,
            tracks,
            skipped,
            default_path,
        } => {
            if let Some(d) = &mut s.export {
                if d.playlist_id == id {
                    d.track_count = tracks;
                    d.skipped = skipped;
                    d.path = default_path;
                    d.format = s.config.last_export_format;
                    d.step = ExportStep::Form;
                }
            }
        }
        Event::ExportPathChosen(path) => {
            if let Some(d) = &mut s.export {
                d.format =
                    crate::core::config::ExportFormat::from_path(std::path::Path::new(&path));
                d.path = path;
            }
        }
        Event::ExportNeedsOverwrite => {
            if let Some(d) = &mut s.export {
                d.step = ExportStep::ConfirmOverwrite;
            }
        }
        Event::Exported { path, tracks } => {
            s.export = None;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            s.toast(
                ToastKind::Success,
                format!("Exported {tracks} tracks to {name}"),
                path.parent().map(std::path::Path::to_path_buf),
            );
        }
        Event::ExportFailed(e) => match &mut s.export {
            Some(d) => {
                d.error = Some(e.into());
                if d.step != ExportStep::Loading || d.track_count > 0 {
                    d.step = ExportStep::Form;
                }
            }
            None => s.error = Some(e.into()),
        },
        Event::RateLimited(secs) => {
            s.rate_limited_until =
                (secs > 0).then(|| Instant::now() + std::time::Duration::from_secs(secs));
        }
        Event::Error(e) => s.toast(ToastKind::Error, e.to_string(), None),
    }
    let _ = out;
}

/// Draw the whole window. Pure UI over `UiState`; also used by the
/// screenshot tests.
pub fn draw(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    egui::Panel::top("top_bar")
        .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 8)))
        .show(ui, |ui| top_bar(ui, s));

    let signed_in = s.spotify.is_connected();
    if signed_in && s.screen.shows_spotify_data() {
        egui::Panel::bottom("attribution_footer")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                ui.label(RichText::new(ATTRIBUTION).color(muted(ui)));
            });
    }

    // Tighter side margins on small windows (800 px minimum).
    let side = if ui.available_width() < 900.0 { 16 } else { 24 };
    egui::CentralPanel::default()
        .frame(
            egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(side, 16)),
        )
        .show(ui, |ui| {
            banners(ui, s);
            match s.screen {
                Screen::Starting => {
                    ui.add_space(80.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.label("Starting…");
                    });
                }
                Screen::Setup => screens::setup::show(ui, s, out),
                Screen::SignIn => screens::signin::show(ui, s, out),
                Screen::Playlists => screens::playlists::show(ui, s, out),
                Screen::Detail => screens::detail::show(ui, s, out),
                Screen::Transfers => screens::transfers::show(ui, s, out),
                Screen::Settings => screens::settings::show(ui, s, out),
                Screen::About => screens::about::show(ui, s, out),
            }
        });

    screens::modals(ui, s, out);
    toasts(ui, s, out);
}

fn top_bar(ui: &mut Ui, s: &mut UiState) {
    ui.horizontal(|ui| {
        widgets::brand_mark(ui, 22.0);
        ui.label(RichText::new("playlist-porter").size(17.0).strong());
        ui.add_space(20.0);
        let signed_in = s.spotify.is_connected();
        // Settings stays reachable right after a disconnect so its report shows.
        if signed_in || s.screen == Screen::Settings {
            let nav = [
                (Screen::Playlists, "Playlists"),
                (Screen::Transfers, "Transfers"),
                (Screen::Settings, "Settings"),
                (Screen::About, "About"),
            ];
            for (screen, label) in nav {
                let active = s.screen == screen
                    || (screen == Screen::Playlists && s.screen == Screen::Detail);
                if ui
                    .selectable_label(active, RichText::new(label).size(15.0))
                    .clicked()
                {
                    s.screen = if signed_in || matches!(screen, Screen::Settings | Screen::About) {
                        screen
                    } else {
                        Screen::SignIn
                    };
                }
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if signed_in {
                // Full "Spotify: name" when there's room, otherwise just the
                // service names (with the account in a tooltip).
                let compact = ui.available_width() < 420.0;
                conn_status(ui, Service::YouTube, &s.youtube, compact);
                ui.add_space(12.0);
                conn_status(ui, Service::Spotify, &s.spotify, compact);
            } else {
                if ui.link("About & privacy").clicked() {
                    s.screen = Screen::About;
                }
                if s.screen != Screen::Setup && s.screen != Screen::Starting {
                    ui.add_space(12.0);
                    if ui.link("Setup").clicked() {
                        s.setup.from_settings = false;
                        s.screen = Screen::Setup;
                    }
                }
                if s.screen == Screen::About && s.config.has_spotify() {
                    ui.add_space(12.0);
                    if ui.link("Sign in").clicked() {
                        s.screen = Screen::SignIn;
                    }
                }
            }
        });
    });
}

/// "● Spotify: Alex" / "○ YouTube: not connected" (symbol + text, not colour alone).
fn conn_status(ui: &mut Ui, service: Service, c: &Conn, compact: bool) {
    let (dot, full, color) = match c {
        Conn::Connected { name, .. } => ("●", format!("{}: {name}", service.label()), ok_color(ui)),
        _ => (
            "○",
            format!("{}: not connected", service.label()),
            muted(ui),
        ),
    };
    let text = if compact {
        service.label().to_string()
    } else {
        full.clone()
    };
    let r = ui.label(RichText::new(text).color(ui.visuals().text_color()));
    let r = r | ui.label(RichText::new(dot).color(color));
    if compact {
        r.on_hover_text(full);
    }
}

fn banners(ui: &mut Ui, s: &mut UiState) {
    if let Some(until) = s.rate_limited_until {
        let left = until.saturating_duration_since(Instant::now()).as_secs();
        if left == 0 {
            s.rate_limited_until = None;
        } else {
            widgets::callout(ui, widgets::warn_color(ui), |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Rate limited, retrying in {left} s"));
                });
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(500));
            ui.add_space(8.0);
        }
    }
    if let Some(e) = &s.keyring_error {
        if widgets::error_panel(ui, e) {
            s.keyring_error = None;
        }
        ui.add_space(8.0);
    }
    if let Some(e) = &s.error {
        if widgets::error_panel(ui, e) {
            s.error = None;
        }
        ui.add_space(8.0);
    }
}

fn toasts(ui: &mut Ui, s: &mut UiState, out: &mut Vec<Command>) {
    let now = Instant::now();
    s.toasts.retain(|t| t.until > now);
    if s.toasts.is_empty() {
        return;
    }
    let ctx = ui.ctx().clone();
    let mut close = None;
    egui::Area::new(egui::Id::unique("toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -48.0])
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            for (i, t) in s.toasts.iter().enumerate() {
                egui::Frame::popup(ui.style())
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.horizontal(|ui| {
                            let (icon, color) = match t.kind {
                                ToastKind::Success => ("✔", ok_color(ui)),
                                ToastKind::Error => ("⚠", widgets::err_color(ui)),
                            };
                            ui.label(RichText::new(icon).color(color).size(16.0));
                            ui.label(&t.text);
                            if let Some(folder) = &t.folder {
                                if ui.button("Show in folder").clicked() {
                                    out.push(Command::OpenFolder(folder.clone()));
                                }
                            }
                            if ui.small_button("✖").on_hover_text("Dismiss").clicked() {
                                close = Some(i);
                            }
                        });
                    });
                ui.add_space(6.0);
            }
        });
    if let Some(i) = close {
        s.toasts.remove(i);
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(1));
}
