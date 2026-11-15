//! Disconnect / sign-out flows. Each runs immediately and reports exactly
//! what was deleted (Terms V.8 / Appendix A.5.c; YouTube III.D.2.c, III.E.4.g).

use super::{blocking, Event, Worker};
use crate::core::auth::google;
use crate::core::auth::keyring::{self, Account};
use crate::core::auth::token_http_client;
use crate::core::youtube::resume::ResumeStore;
use crate::gui::state::DisconnectReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disconnect {
    Spotify,
    YouTube,
    Everything,
}

impl Worker {
    pub(super) async fn disconnect(&self, which: Disconnect) {
        let mut r = DisconnectReport {
            title: match which {
                Disconnect::Spotify => "Spotify disconnected",
                Disconnect::YouTube => "YouTube disconnected",
                Disconnect::Everything => "Signed out of everything",
            }
            .into(),
            spotify_disconnected: which != Disconnect::YouTube,
            ..Default::default()
        };

        // Stop any running transfer first so it can't write new state.
        let (yt_client, cancel) = {
            let mut st = self.st.lock().await;
            (st.youtube.clone(), st.transfer_cancel.take())
        };
        if let Some(c) = cancel {
            c.cancel();
            r.done.push("Stopped the running YouTube transfer.".into());
        }

        if which != Disconnect::Spotify {
            // Revoke the Google grant programmatically, then delete the token.
            let token = match &yt_client {
                Some(c) => Some(c.session().snapshot().await),
                None => keyring::load(Account::Google).await.ok().flatten(),
            };
            match token {
                Some(t) => match google::revoke(&token_http_client(), &t).await {
                    Ok(()) => r
                        .done
                        .push("Revoked playlist-porter's access to your Google account.".into()),
                    Err(e) => r.problems.push(format!(
                        "Couldn't revoke Google access ({e}). Remove it in your Google Account permissions."
                    )),
                },
                None => r.done.push("No Google token was stored, so nothing needed revoking.".into()),
            }
            match keyring::delete(Account::Google).await {
                Ok(true) => r.done.push(
                    "Deleted the Google sign-in token from your OS keyring (playlist-porter / google)."
                        .into(),
                ),
                Ok(false) => {}
                Err(e) => r.problems.push(format!("Couldn't delete the Google token: {e}")),
            }
            let mut st = self.st.lock().await;
            st.youtube = None;
            st.transfer_data = None;
            if let Some(c) = st.google_signin.take() {
                c.cancel();
            }
        }

        if which != Disconnect::YouTube {
            match keyring::delete(Account::Spotify).await {
                Ok(true) => r.done.push(
                    "Deleted the Spotify sign-in token from your OS keyring (playlist-porter / spotify)."
                        .into(),
                ),
                Ok(false) => r.done.push("No Spotify token was stored.".into()),
                Err(e) => r.problems.push(format!("Couldn't delete the Spotify token: {e}")),
            }
            let mut st = self.st.lock().await;
            st.spotify = None;
            st.me = None;
            st.export_data = None;
            st.transfer_data = None;
            if let Some(c) = st.spotify_signin.take() {
                c.cancel();
            }
        }

        // Resume state ties a Spotify playlist to a YouTube playlist, so it
        // goes with either account.
        match blocking(|| ResumeStore::open_default()?.delete_all()).await {
            Ok(0) => r.done.push("No paused-transfer files were saved.".into()),
            Ok(n) => r.done.push(format!(
                "Deleted {n} paused-transfer file{} and any temporary files.",
                if n == 1 { "" } else { "s" }
            )),
            Err(e) => r
                .problems
                .push(format!("Couldn't delete paused-transfer files: {e}")),
        }

        r.done.push(match which {
            Disconnect::Spotify => "Cleared your loaded playlists and tracks from memory.".into(),
            Disconnect::YouTube => "Cleared the transfer report from memory.".into(),
            Disconnect::Everything => {
                "Cleared all loaded playlists, tracks and transfer data from memory.".into()
            }
        });

        self.sink.send(Event::Disconnected(which, r));
        self.list_resumes().await;
    }
}
