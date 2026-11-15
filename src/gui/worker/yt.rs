//! Worker side of the YouTube feature: Google sign-in, transfers, resume
//! state and the match report.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use super::{blocking, Event, TransferPrep, Worker};
use crate::core::auth::google::{self as google_auth, GoogleApp};
use crate::core::auth::keyring::{self, Account};
use crate::core::auth::session::{RefreshFn, Session};
use crate::core::auth::{loopback, token_http_client, StoredToken};
use crate::core::config::{Config, ExportFormat};
use crate::core::error::{CoreError, CoreResult};
use crate::core::export::{sanitize_file_name, write_file};
use crate::core::spotify::models::SkipCounts;
use crate::core::youtube::client::YouTubeClient;
use crate::core::youtube::quota::{self, COST_CHANNELS};
use crate::core::youtube::report::{self, ReportRow};
use crate::core::youtube::resume::ResumeStore;
use crate::core::youtube::transfer::{self, Outcome, TransferJob};
use crate::gui::state::Service;

impl Worker {
    fn google_client(&self, cfg: &Config, token: StoredToken) -> Arc<YouTubeClient> {
        let id = cfg.google_client_id.clone();
        let secret = cfg.google_client_secret.clone();
        let token_http = token_http_client();
        let refresher: RefreshFn = Box::new(move |rt| {
            let (id, secret, token_http) = (id.clone(), secret.clone(), token_http.clone());
            Box::pin(async move { google_auth::refresh(&token_http, &id, &secret, &rt).await })
        });
        let session = Arc::new(Session::new(Account::Google, token, refresher));
        Arc::new(YouTubeClient::new(self.http.clone(), session))
    }

    pub(super) fn quota_status(&self) {
        let now = Utc::now();
        let used = self.quota.lock().map(|mut q| q.used(now)).unwrap_or(0);
        self.sink.send(Event::QuotaStatus {
            used,
            reset_local: quota::reset_local_label(now),
        });
    }

    fn spend(&self, units: u32) {
        if let Ok(mut q) = self.quota.lock() {
            q.spend(units, Utc::now());
        }
        self.quota_status();
    }

    /// Fetch the channel name and publish the YouTube session.
    async fn publish_google(&self, client: Arc<YouTubeClient>) -> CoreResult<()> {
        self.spend(COST_CHANNELS);
        let name = match client.channel_title().await {
            Ok(n) => n,
            Err(e @ (CoreError::SessionExpired | CoreError::Auth(_))) => return Err(e),
            Err(CoreError::YouTubeQuotaExceeded) => {
                if let Ok(mut q) = self.quota.lock() {
                    q.exhaust(Utc::now());
                }
                self.quota_status();
                "YouTube account".to_string()
            }
            // Network trouble etc.: the token is fine, the name can wait.
            Err(_) => "YouTube account".to_string(),
        };
        self.st.lock().await.youtube = Some(client);
        self.sink.send(Event::SignedIn {
            service: Service::YouTube,
            name,
            id: String::new(),
        });
        Ok(())
    }

    pub(super) async fn restore_google(&self) {
        let token = match keyring::load(Account::Google).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                self.sink.send(Event::SignedOut(Service::YouTube, None));
                return;
            }
            Err(e) => {
                self.sink.send(Event::KeyringUnavailable(e));
                self.sink.send(Event::SignedOut(Service::YouTube, None));
                return;
            }
        };
        let cfg = self.st.lock().await.cfg.clone();
        if !cfg.has_google() {
            // Can't refresh without the app credentials; keep it signed out.
            self.sink.send(Event::SignedOut(Service::YouTube, None));
            return;
        }
        let client = self.google_client(&cfg, token);
        if self.publish_google(client).await.is_err() {
            let _ = keyring::delete(Account::Google).await;
            self.sink.send(Event::SignedOut(Service::YouTube, None));
        }
    }

    pub(super) async fn google_sign_in(&self) {
        let cancel = CancellationToken::new();
        let cfg = {
            let mut st = self.st.lock().await;
            if let Some(old) = st.google_signin.replace(cancel.clone()) {
                old.cancel();
            }
            st.cfg.clone()
        };
        let res = self.google_sign_in_inner(&cfg, cancel).await;
        self.st.lock().await.google_signin = None;
        let res = match res {
            Ok(client) => self.publish_google(client).await,
            Err(e) => Err(e),
        };
        if let Err(e) = res {
            self.sink.send(Event::SignInFailed(Service::YouTube, e));
        }
    }

    async fn google_sign_in_inner(
        &self,
        cfg: &Config,
        cancel: CancellationToken,
    ) -> CoreResult<Arc<YouTubeClient>> {
        if !cfg.has_google() {
            return Err(CoreError::Config(
                "the Google client ID and secret are missing".into(),
            ));
        }
        if let Err(e) = keyring::load(Account::Google).await {
            self.sink.send(Event::KeyringUnavailable(e.clone()));
            return Err(e);
        }
        let redirect = cfg.redirect_uri();
        let app = GoogleApp {
            client_id: &cfg.google_client_id,
            client_secret: &cfg.google_client_secret,
            redirect_uri: &redirect,
        };
        let pending = google_auth::begin(&app)?;
        let listener = loopback::bind(cfg.redirect_port()).await?;
        self.sink.send(Event::SignInWaiting {
            service: Service::YouTube,
            auth_url: pending.auth_url.clone(),
            port: cfg.redirect_port(),
        });
        let url = pending.auth_url.clone();
        let _ = tokio::task::spawn_blocking(move || open::that_detached(url)).await;
        let code = loopback::wait_for_code(listener, pending.csrf_state, cancel).await?;
        self.sink.send(Event::SignInFinishing(Service::YouTube));
        let token =
            google_auth::exchange(&token_http_client(), &app, code, pending.verifier).await?;
        keyring::save(Account::Google, token.clone()).await?;
        Ok(self.google_client(cfg, token))
    }

    pub(super) async fn list_resumes(&self) {
        let res =
            blocking(|| ResumeStore::open_default()?.purge_and_list(std::time::SystemTime::now()))
                .await;
        match res {
            Ok(list) => self.sink.send(Event::ResumeList(list)),
            Err(e) => self.sink.send(Event::Error(e)),
        }
    }

    pub(super) async fn discard_resume(&self, id: String) {
        let _ = blocking(move || ResumeStore::open_default()?.delete(&id)).await;
        self.list_resumes().await;
    }

    pub(super) async fn prepare_transfer(&self, id: String, resume: bool) {
        let res = async {
            let (client, me) = self.current_spotify().await?;
            // Re-fetch from Spotify (also on resume) and re-check ownership.
            let (playlist, entries) = client.load_for_processing(&id, &me).await?;
            let resume_state = if resume {
                let pid = id.clone();
                blocking(move || {
                    ResumeStore::open_default()?.load(&pid, std::time::SystemTime::now())
                })
                .await?
            } else {
                None
            };
            Ok::<_, CoreError>((playlist, entries, resume_state))
        }
        .await;
        match res {
            Ok((playlist, entries, resume_state)) => {
                let skipped = SkipCounts::of(&entries);
                let tracks: Vec<_> = entries.iter().filter_map(|e| e.track().cloned()).collect();
                let done = resume_state
                    .as_ref()
                    .map(|r| r.next_index.min(tracks.len()))
                    .unwrap_or(0);
                let resuming = resume_state
                    .as_ref()
                    .is_some_and(|r| !r.youtube_playlist_id.is_empty());
                let now = Utc::now();
                let used = self.quota.lock().map(|mut q| q.used(now)).unwrap_or(0);
                self.sink.send(Event::TransferReady {
                    id,
                    tracks: tracks.len() - done,
                    total_tracks: tracks.len(),
                    skipped,
                    resuming,
                    quota_used: used,
                    reset_local: quota::reset_local_label(now),
                });
                self.st.lock().await.transfer_data = Some(TransferPrep {
                    playlist,
                    tracks,
                    resume: resume_state,
                });
            }
            Err(e) => {
                let e = self.spotify_error(e).await;
                self.sink.send(Event::TransferFailedToStart(e));
            }
        }
    }

    pub(super) async fn start_transfer(&self) {
        let cancel = CancellationToken::new();
        let (prep, client) = {
            let mut st = self.st.lock().await;
            if st.transfer_cancel.is_some() {
                return; // one transfer at a time
            }
            let prep = st.transfer_data.take();
            if prep.is_some() {
                st.transfer_cancel = Some(cancel.clone());
            }
            (prep, st.youtube.clone())
        };
        let (Some(prep), Some(client)) = (prep, client) else {
            self.st.lock().await.transfer_cancel = None;
            self.sink
                .send(Event::TransferFailedToStart(CoreError::YouTube(
                    "the transfer isn't ready; start it again from the playlist".into(),
                )));
            return;
        };
        let store = match ResumeStore::open_default() {
            Ok(s) => s,
            Err(e) => {
                self.st.lock().await.transfer_cancel = None;
                self.sink.send(Event::TransferFailedToStart(e));
                return;
            }
        };
        let job = TransferJob {
            spotify_playlist_id: prep.playlist.id.clone(),
            playlist_name: prep.playlist.name.clone(),
            playlist_url: prep.playlist.spotify_url(),
            tracks: prep.tracks,
            resume: prep.resume,
        };
        let sink = self.sink.clone();
        let outcome = transfer::run(
            &*client,
            job,
            &self.quota,
            Some(&store),
            &cancel,
            move |p| sink.send(Event::TransferProgress(p)),
            Utc::now,
        )
        .await;
        self.st.lock().await.transfer_cancel = None;
        if let Outcome::Failed {
            error: CoreError::SessionExpired,
            ..
        } = &outcome
        {
            self.st.lock().await.youtube = None;
            let _ = keyring::delete(Account::Google).await;
            self.sink.send(Event::SignedOut(
                Service::YouTube,
                Some(CoreError::SessionExpired),
            ));
        }
        self.quota_status();
        self.sink.send(Event::TransferEnded(outcome));
        self.list_resumes().await;
    }

    pub(super) async fn save_report(
        &self,
        playlist_name: String,
        playlist_url: String,
        youtube_playlist_id: Option<String>,
        rows: Vec<ReportRow>,
    ) {
        let default_dir = self
            .st
            .lock()
            .await
            .cfg
            .last_export_dir
            .clone()
            .or_else(dirs::document_dir);
        let mut dlg = rfd::AsyncFileDialog::new()
            .set_title("Save match report")
            .set_file_name(format!(
                "{} - match report.csv",
                sanitize_file_name(&playlist_name)
            ))
            .add_filter("CSV", &["csv"])
            .add_filter("JSON", &["json"]);
        if let Some(d) = default_dir.filter(|d| d.is_dir()) {
            dlg = dlg.set_directory(d);
        }
        let Some(handle) = dlg.save_file().await else {
            return;
        };
        let mut path: PathBuf = handle.path().to_path_buf();
        if path.extension().is_none() {
            path.set_extension("csv");
        }
        let format = ExportFormat::from_path(&path);
        let target = path.clone();
        // The native dialog already asked about replacing an existing file.
        let res = blocking(move || {
            let body =
                report::render(format, &playlist_url, youtube_playlist_id.as_deref(), &rows)?;
            write_file(&target, &body, true)
        })
        .await;
        match res {
            Ok(()) => self.sink.send(Event::ReportSaved(path)),
            Err(e) => self.sink.send(Event::Error(e)),
        }
    }
}
