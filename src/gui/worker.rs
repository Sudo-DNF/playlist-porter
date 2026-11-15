//! Bridge between the UI thread and the tokio runtime.
//!
//! The UI sends [`Command`]s over an unbounded tokio channel; the worker runs
//! all network, OAuth and file I/O on the runtime and sends [`Event`]s back
//! over a std channel, calling `ctx.request_repaint()` after each one. The UI
//! drains events with `try_recv` once per frame and never blocks.

use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use tokio::runtime::Handle;
use tokio::sync::{mpsc as tmpsc, Mutex};
use tokio_util::sync::CancellationToken;

use super::state::{DisconnectReport, PlaylistRow, Service};
use crate::core::auth::keyring::{self, Account};
use crate::core::auth::session::Session;
use crate::core::auth::{loopback, spotify as spotify_auth, StoredToken};
use crate::core::config::{self, Config, EnvOverrides, ExportFormat};
use crate::core::error::{CoreError, CoreResult};
use crate::core::export;
use crate::core::spotify::client::{is_exportable, SpotifyClient};
use crate::core::spotify::models::{Entry, Playlist, SkipCounts, User};
use crate::core::youtube::client::YouTubeClient;
use crate::core::youtube::quota::QuotaTracker;
use crate::core::youtube::report::ReportRow;
use crate::core::youtube::resume::ResumeState;
use crate::core::youtube::transfer::{Outcome, Progress};

mod account;
mod yt;

pub use account::Disconnect;

/// Requests from the UI.
#[derive(Debug)]
pub enum Command {
    /// Load config, check the keyring, restore sessions.
    Startup,
    /// Write `config.toml` (file values only) and reload the merged config.
    SaveConfig(Box<Config>),
    SignIn(Service),
    CancelSignIn(Service),
    LoadPlaylists,
    OpenDetail(String),
    OpenFolder(std::path::PathBuf),
    /// Fetch a playlist's tracks for the export dialog.
    PrepareExport(String),
    /// Native save dialog.
    ChooseExportPath {
        current: String,
        format: ExportFormat,
    },
    WriteExport {
        id: String,
        path: String,
        format: ExportFormat,
        overwrite: bool,
    },
    /// Export dialog closed: drop the fetched tracks.
    CloseExport,
    /// Fetch a playlist for the YouTube confirmation modal.
    PrepareTransfer {
        id: String,
        resume: bool,
    },
    /// Start (or resume) the prepared transfer.
    StartTransfer,
    /// Stop after the current API call and save resume state.
    CancelTransfer,
    /// Confirmation dismissed: drop the fetched tracks.
    CloseTransfer,
    /// "Start over": delete saved resume state for a playlist.
    DiscardResume(String),
    /// Native save dialog, then write the match report.
    SaveReport {
        playlist_name: String,
        playlist_url: String,
        youtube_playlist_id: Option<String>,
        rows: Vec<ReportRow>,
    },
    /// Disconnect Spotify, YouTube, or sign out of everything.
    Disconnect(Disconnect),
}

/// Results and progress from the worker.
#[derive(Debug)]
pub enum Event {
    ConfigLoaded {
        merged: Box<Config>,
        file: Box<Config>,
        env: EnvOverrides,
    },
    ConfigSaved,
    /// No stored session for this service (or it was removed).
    SignedOut(Service, Option<CoreError>),
    SignInWaiting {
        service: Service,
        auth_url: String,
        port: u16,
    },
    SignInFinishing(Service),
    SignedIn {
        service: Service,
        name: String,
        id: String,
    },
    SignInFailed(Service, CoreError),
    KeyringUnavailable(CoreError),
    PlaylistsLoading,
    PlaylistsLoaded(Vec<PlaylistRow>),
    PlaylistsFailed(CoreError),
    DetailLoaded {
        row: PlaylistRow,
        entries: Vec<Entry>,
    },
    DetailFailed(String, CoreError),
    ExportReady {
        id: String,
        tracks: usize,
        skipped: SkipCounts,
        default_path: String,
    },
    ExportPathChosen(String),
    ExportNeedsOverwrite,
    Exported {
        path: PathBuf,
        tracks: usize,
    },
    ExportFailed(CoreError),
    /// Playlists with saved resume state: (Spotify playlist ID, next index).
    ResumeList(Vec<(String, usize)>),
    TransferReady {
        id: String,
        /// Tracks still to process.
        tracks: usize,
        total_tracks: usize,
        skipped: SkipCounts,
        resuming: bool,
        quota_used: u32,
        reset_local: String,
    },
    TransferProgress(Progress),
    TransferEnded(Outcome),
    TransferFailedToStart(CoreError),
    QuotaStatus {
        used: u32,
        reset_local: String,
    },
    ReportSaved(PathBuf),
    Disconnected(Disconnect, DisconnectReport),
    /// Spotify asked us to wait; 0 means the wait is over.
    RateLimited(u64),
    Error(CoreError),
}

/// Sends events to the UI and wakes it up.
#[derive(Clone)]
pub struct Sink {
    tx: mpsc::Sender<Event>,
    ctx: egui::Context,
}

impl Sink {
    pub fn send(&self, ev: Event) {
        // The receiver only disappears when the window is closing.
        let _ = self.tx.send(ev);
        self.ctx.request_repaint();
    }
}

pub struct WorkerHandle {
    tx: tmpsc::UnboundedSender<Command>,
    pub events: mpsc::Receiver<Event>,
}

impl WorkerHandle {
    pub fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }
}

/// Everything the worker keeps between commands. In memory only.
#[derive(Default)]
struct WorkerState {
    cfg: Config,
    spotify: Option<Arc<SpotifyClient>>,
    me: Option<User>,
    spotify_signin: Option<CancellationToken>,
    google_signin: Option<CancellationToken>,
    /// Tracks fetched for the open export dialog. Dropped when it closes.
    export_data: Option<(Playlist, Vec<Entry>)>,
    youtube: Option<Arc<YouTubeClient>>,
    /// Playlist + tracks fetched for the YouTube confirmation, in memory only.
    transfer_data: Option<TransferPrep>,
    transfer_cancel: Option<CancellationToken>,
}

struct TransferPrep {
    playlist: Playlist,
    tracks: Vec<crate::core::spotify::models::TrackInfo>,
    resume: Option<ResumeState>,
}

#[derive(Clone)]
struct Worker {
    sink: Sink,
    http: reqwest::Client,
    st: Arc<Mutex<WorkerState>>,
    /// YouTube quota estimate for today, shared across transfers.
    quota: Arc<std::sync::Mutex<QuotaTracker>>,
}

/// One HTTP client for all API calls. Redirects are off so a request can
/// never be bounced to a host outside the allowed list.
fn api_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(concat!("playlist-porter/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_default()
}

/// Start the worker loop on the app-owned runtime.
pub fn spawn(rt: &Handle, ctx: egui::Context) -> WorkerHandle {
    let (cmd_tx, mut cmd_rx) = tmpsc::unbounded_channel::<Command>();
    let (ev_tx, ev_rx) = mpsc::channel();
    let w = Worker {
        sink: Sink { tx: ev_tx, ctx },
        http: api_http_client(),
        st: Arc::new(Mutex::new(WorkerState::default())),
        quota: Arc::new(std::sync::Mutex::new(QuotaTracker::new(chrono::Utc::now()))),
    };
    rt.spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            // Each command runs in its own task so a slow one never holds up
            // cancellation or other requests.
            let w = w.clone();
            tokio::spawn(async move { w.handle(cmd).await });
        }
    });
    WorkerHandle {
        tx: cmd_tx,
        events: ev_rx,
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Io(e.to_string()))?
}

pub fn playlist_row(p: &Playlist, me_id: &str) -> PlaylistRow {
    PlaylistRow {
        id: p.id.clone(),
        name: p.name.clone(),
        owner_name: p.owner.name().to_string(),
        owner_is_me: p.owner.id == me_id,
        collaborative: p.collaborative,
        total: p.total(),
        spotify_url: p.spotify_url(),
        exportable: is_exportable(&p.owner.id, p.collaborative, me_id),
    }
}

impl Worker {
    async fn handle(&self, cmd: Command) {
        match cmd {
            Command::Startup => self.startup().await,
            Command::SaveConfig(cfg) => self.save_config(*cfg).await,
            Command::SignIn(Service::Spotify) => self.spotify_sign_in().await,
            Command::SignIn(Service::YouTube) => self.google_sign_in().await,
            Command::CancelSignIn(s) => {
                let mut st = self.st.lock().await;
                let tok = match s {
                    Service::Spotify => st.spotify_signin.take(),
                    Service::YouTube => st.google_signin.take(),
                };
                if let Some(t) = tok {
                    t.cancel();
                }
            }
            Command::LoadPlaylists => self.load_playlists().await,
            Command::OpenDetail(id) => self.open_detail(id).await,
            Command::PrepareExport(id) => self.prepare_export(id).await,
            Command::ChooseExportPath { current, format } => {
                self.choose_export_path(current, format).await
            }
            Command::WriteExport {
                id,
                path,
                format,
                overwrite,
            } => self.write_export(id, path, format, overwrite).await,
            Command::CloseExport => self.st.lock().await.export_data = None,
            Command::PrepareTransfer { id, resume } => self.prepare_transfer(id, resume).await,
            Command::StartTransfer => self.start_transfer().await,
            Command::CancelTransfer => {
                if let Some(t) = &self.st.lock().await.transfer_cancel {
                    t.cancel();
                }
            }
            Command::CloseTransfer => self.st.lock().await.transfer_data = None,
            Command::DiscardResume(id) => self.discard_resume(id).await,
            Command::Disconnect(which) => self.disconnect(which).await,
            Command::SaveReport {
                playlist_name,
                playlist_url,
                youtube_playlist_id,
                rows,
            } => {
                self.save_report(playlist_name, playlist_url, youtube_playlist_id, rows)
                    .await
            }
            Command::OpenFolder(path) => {
                let _ = blocking(move || {
                    open::that_detached(&path).map_err(|e| CoreError::Io(e.to_string()))
                })
                .await;
            }
        }
    }

    async fn load_config(&self) -> CoreResult<Config> {
        let (merged, file, env) = blocking(config::load).await?;
        self.st.lock().await.cfg = merged.clone();
        self.sink.send(Event::ConfigLoaded {
            merged: Box::new(merged.clone()),
            file: Box::new(file),
            env,
        });
        Ok(merged)
    }

    async fn startup(&self) {
        let cfg = match self.load_config().await {
            Ok(c) => c,
            Err(e) => {
                self.sink.send(Event::Error(e));
                return;
            }
        };
        self.list_resumes().await;
        if !cfg.has_spotify() {
            return;
        }
        self.restore_google().await;
        self.restore_spotify().await;
    }

    async fn save_config(&self, file_cfg: Config) {
        let res = blocking(move || config::save(&file_cfg)).await;
        match res {
            Ok(()) => {
                self.sink.send(Event::ConfigSaved);
                if let Ok(cfg) = self.load_config().await {
                    let signed_in = self.st.lock().await.spotify.is_some();
                    if cfg.has_spotify() && !signed_in {
                        self.restore_spotify().await;
                    }
                }
            }
            Err(e) => self.sink.send(Event::Error(e)),
        }
    }

    fn spotify_client(&self, cfg: &Config, token: StoredToken) -> Arc<SpotifyClient> {
        let client_id = cfg.spotify_client_id.clone();
        let token_http = crate::core::auth::token_http_client();
        let refresher: crate::core::auth::session::RefreshFn = Box::new(move |rt| {
            let client_id = client_id.clone();
            let token_http = token_http.clone();
            Box::pin(async move { spotify_auth::refresh(&token_http, &client_id, &rt).await })
        });
        let session = Arc::new(Session::new(Account::Spotify, token, refresher));
        let sink = self.sink.clone();
        Arc::new(SpotifyClient::new(
            self.http.clone(),
            session,
            Arc::new(move |secs| sink.send(Event::RateLimited(secs))),
        ))
    }

    /// Restore a stored Spotify session, if any.
    async fn restore_spotify(&self) {
        let token = match keyring::load(Account::Spotify).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                self.sink.send(Event::SignedOut(Service::Spotify, None));
                return;
            }
            Err(e) => {
                self.sink.send(Event::KeyringUnavailable(e.clone()));
                self.sink.send(Event::SignedOut(Service::Spotify, None));
                return;
            }
        };
        let cfg = self.st.lock().await.cfg.clone();
        let client = self.spotify_client(&cfg, token);
        self.finish_spotify_sign_in(client, false).await;
    }

    /// Fetch the profile, publish the session and load playlists.
    async fn finish_spotify_sign_in(&self, client: Arc<SpotifyClient>, fresh: bool) {
        match client.me().await {
            Ok(me) => {
                {
                    let mut st = self.st.lock().await;
                    st.spotify = Some(client);
                    st.me = Some(me.clone());
                }
                self.sink.send(Event::SignedIn {
                    service: Service::Spotify,
                    name: me.name().to_string(),
                    id: me.id.clone(),
                });
                self.load_playlists().await;
            }
            Err(e) => {
                // A rejected or expired session is useless: remove it.
                if matches!(
                    e,
                    CoreError::SessionExpired | CoreError::SpotifyForbidden { .. }
                ) {
                    let _ = keyring::delete(Account::Spotify).await;
                }
                if fresh || !matches!(e, CoreError::SessionExpired) {
                    self.sink.send(Event::SignInFailed(Service::Spotify, e));
                } else {
                    self.sink.send(Event::SignedOut(Service::Spotify, Some(e)));
                }
            }
        }
    }

    async fn spotify_sign_in(&self) {
        let cancel = CancellationToken::new();
        let cfg = {
            let mut st = self.st.lock().await;
            if let Some(old) = st.spotify_signin.replace(cancel.clone()) {
                old.cancel();
            }
            st.cfg.clone()
        };
        let res = self.spotify_sign_in_inner(&cfg, cancel).await;
        self.st.lock().await.spotify_signin = None;
        match res {
            Ok(client) => self.finish_spotify_sign_in(client, true).await,
            Err(e) => self.sink.send(Event::SignInFailed(Service::Spotify, e)),
        }
    }

    async fn spotify_sign_in_inner(
        &self,
        cfg: &Config,
        cancel: CancellationToken,
    ) -> CoreResult<Arc<SpotifyClient>> {
        if !cfg.has_spotify() {
            return Err(CoreError::Config("the Spotify client ID is missing".into()));
        }
        // Make sure tokens can be stored before asking the user to sign in.
        if let Err(e) = keyring::load(Account::Spotify).await {
            self.sink.send(Event::KeyringUnavailable(e.clone()));
            return Err(e);
        }
        let redirect = cfg.redirect_uri();
        let pending = spotify_auth::begin(&cfg.spotify_client_id, &redirect)?;
        let listener = loopback::bind(cfg.redirect_port()).await?;
        self.sink.send(Event::SignInWaiting {
            service: Service::Spotify,
            auth_url: pending.auth_url.clone(),
            port: cfg.redirect_port(),
        });
        // Open the system browser; if that fails the UI shows the URL to copy.
        let url = pending.auth_url.clone();
        let _ = tokio::task::spawn_blocking(move || open::that_detached(url)).await;
        let code = loopback::wait_for_code(listener, pending.csrf_state, cancel).await?;
        self.sink.send(Event::SignInFinishing(Service::Spotify));
        let token = spotify_auth::exchange(
            &crate::core::auth::token_http_client(),
            &cfg.spotify_client_id,
            &redirect,
            code,
            pending.verifier,
        )
        .await?;
        keyring::save(Account::Spotify, token.clone()).await?;
        Ok(self.spotify_client(cfg, token))
    }

    async fn current_spotify(&self) -> CoreResult<(Arc<SpotifyClient>, User)> {
        let st = self.st.lock().await;
        match (&st.spotify, &st.me) {
            (Some(c), Some(m)) => Ok((c.clone(), m.clone())),
            _ => Err(CoreError::SessionExpired),
        }
    }

    /// React to an error from a Spotify call; returns it for display.
    async fn spotify_error(&self, e: CoreError) -> CoreError {
        if matches!(e, CoreError::SessionExpired) {
            {
                let mut st = self.st.lock().await;
                st.spotify = None;
                st.me = None;
            }
            let _ = keyring::delete(Account::Spotify).await;
            self.sink
                .send(Event::SignedOut(Service::Spotify, Some(e.clone())));
        }
        e
    }

    async fn load_playlists(&self) {
        let (client, me) = match self.current_spotify().await {
            Ok(x) => x,
            Err(e) => {
                self.sink.send(Event::PlaylistsFailed(e));
                return;
            }
        };
        self.sink.send(Event::PlaylistsLoading);
        match client.my_playlists().await {
            Ok(list) => {
                let rows = list.iter().map(|p| playlist_row(p, &me.id)).collect();
                self.sink.send(Event::PlaylistsLoaded(rows));
            }
            Err(e) => {
                let e = self.spotify_error(e).await;
                self.sink.send(Event::PlaylistsFailed(e));
            }
        }
    }

    async fn open_detail(&self, id: String) {
        let res = async {
            let (client, me) = self.current_spotify().await?;
            let (p, entries) = client.load_for_processing(&id, &me.id).await?;
            Ok::<_, CoreError>((playlist_row(&p, &me.id), entries))
        }
        .await;
        match res {
            Ok((row, entries)) => self.sink.send(Event::DetailLoaded { row, entries }),
            Err(e) => {
                let e = self.spotify_error(e).await;
                self.sink.send(Event::DetailFailed(id, e));
            }
        }
    }

    async fn prepare_export(&self, id: String) {
        let res = async {
            let (client, me) = self.current_spotify().await?;
            client.load_for_processing(&id, &me.id).await
        }
        .await;
        match res {
            Ok((p, entries)) => {
                let tracks = entries.iter().filter(|e| e.track().is_some()).count();
                let skipped = SkipCounts::of(&entries);
                let cfg = {
                    let mut st = self.st.lock().await;
                    st.export_data = Some((p.clone(), entries));
                    st.cfg.clone()
                };
                let name = p.name.clone();
                let path = tokio::task::spawn_blocking(move || {
                    export::default_path(
                        &name,
                        cfg.last_export_format,
                        cfg.last_export_dir.as_deref(),
                    )
                })
                .await
                .unwrap_or_default();
                self.sink.send(Event::ExportReady {
                    id,
                    tracks,
                    skipped,
                    default_path: path.display().to_string(),
                });
            }
            Err(e) => {
                let e = self.spotify_error(e).await;
                self.sink.send(Event::ExportFailed(e));
            }
        }
    }

    async fn choose_export_path(&self, current: String, format: ExportFormat) {
        let current = export::expand_tilde(&current);
        let mut dlg = rfd::AsyncFileDialog::new().set_title("Export playlist");
        if let Some(dir) = current.parent().filter(|d| d.is_dir()) {
            dlg = dlg.set_directory(dir);
        }
        if let Some(name) = current.file_name() {
            dlg = dlg.set_file_name(name.to_string_lossy());
        }
        dlg = match format {
            ExportFormat::Csv => dlg.add_filter("CSV", &["csv"]),
            ExportFormat::Json => dlg.add_filter("JSON", &["json"]),
        };
        if let Some(handle) = dlg.save_file().await {
            let mut path = handle.path().to_path_buf();
            if path.extension().is_none() {
                path.set_extension(format.extension());
            }
            self.sink
                .send(Event::ExportPathChosen(path.display().to_string()));
        }
    }

    async fn write_export(&self, id: String, path: String, format: ExportFormat, overwrite: bool) {
        let (data, me) = {
            let st = self.st.lock().await;
            (st.export_data.clone(), st.me.clone())
        };
        let (Some((playlist, entries)), Some(me)) = (data, me) else {
            self.sink
                .send(Event::ExportFailed(CoreError::SessionExpired));
            return;
        };
        if playlist.id != id {
            self.sink.send(Event::ExportFailed(CoreError::Io(
                "the export dialog is out of date; close it and try again".into(),
            )));
            return;
        }
        let path = export::expand_tilde(path.trim());
        let target = path.clone();
        let res = blocking(move || {
            export::export_playlist(&playlist, &entries, &me.id, format, &target, overwrite)
        })
        .await;
        match res {
            Ok(tracks) => {
                self.st.lock().await.export_data = None;
                self.sink.send(Event::Exported {
                    path: path.clone(),
                    tracks,
                });
                self.remember_export_prefs(format, path.parent().map(PathBuf::from))
                    .await;
            }
            Err(CoreError::FileExists(_)) => self.sink.send(Event::ExportNeedsOverwrite),
            Err(e) => self.sink.send(Event::ExportFailed(e)),
        }
    }

    /// Save the last export format and folder (UI prefs only) to config.toml.
    async fn remember_export_prefs(&self, format: ExportFormat, dir: Option<PathBuf>) {
        let res = blocking(move || {
            let path = config::config_path()
                .ok_or_else(|| CoreError::Config("no config directory".into()))?;
            let mut file = config::load_file(&path)?;
            file.last_export_format = format;
            if dir.is_some() {
                file.last_export_dir = dir;
            }
            config::save_file(&path, &file)
        })
        .await;
        if res.is_ok() {
            let _ = self.load_config().await;
        }
    }
}
