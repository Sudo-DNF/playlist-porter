//! Error type for everything in `core`. Messages are user-facing and must
//! never contain tokens, auth codes or raw response bodies.

use thiserror::Error;

/// Section of the README that explains Spotify app setup.
pub const README_SETUP_URL: &str = "https://github.com/Sudo-DNF/playlist-porter#setup";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("Configuration problem: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error(
        "Spotify refused access. Either your account isn't in this app's user list \
         (Developer Dashboard → User Management), or the app owner's Premium \
         subscription has lapsed. Spotify said: \"{spotify_message}\""
    )]
    SpotifyForbidden { spotify_message: String },

    #[error("Spotify's request quota for this app is used up (QUOTA_EXCEEDED). Try again later.")]
    SpotifyQuotaExceeded,

    #[error("Spotify kept rate-limiting requests. Wait a minute and try again.")]
    RateLimitGaveUp,

    #[error("Your session has expired and couldn't be renewed. Please sign in again.")]
    SessionExpired,

    #[error(
        "playlist-porter now also reads your Liked Songs, which needs one more \
         permission. Connect Spotify again to grant it."
    )]
    ReauthNeeded,

    #[error("Sign-in failed: {0}")]
    Auth(String),

    #[error("Sign-in was cancelled.")]
    AuthCancelled,

    #[error("Sign-in timed out after 5 minutes without a response from the browser.")]
    AuthTimeout,

    #[error(
        "Couldn't listen on 127.0.0.1:{port} for the sign-in callback ({reason}). \
         Close whatever is using that port, or pick another redirect port in Settings \
         and register the new redirect URI."
    )]
    LoopbackBind { port: u16, reason: String },

    #[error(
        "No system keyring is available ({0}). playlist-porter stores sign-in tokens only \
         in the OS keyring and never in plain files. On Linux, install and unlock a Secret \
         Service provider such as GNOME Keyring or KWallet."
    )]
    NoKeyring(String),

    #[error("Keyring error: {0}")]
    Keyring(String),

    #[error(
        "This playlist can't be exported. Spotify only lets apps move playlists you own or \
         collaborate on."
    )]
    NotExportable,

    #[error("File error: {0}")]
    Io(String),

    #[error("{0} already exists.")]
    FileExists(String),

    #[error(
        "Today's YouTube quota is used up. Progress is saved; resume after the daily reset \
         at midnight Pacific time."
    )]
    YouTubeQuotaExceeded,

    #[error("YouTube error: {0}")]
    YouTube(String),

    #[error("{service} returned HTTP {status}: {message}")]
    Http {
        service: &'static str,
        status: u16,
        message: String,
    },

    #[error("Cancelled.")]
    Cancelled,
}

impl CoreError {
    /// A short, actionable hint shown under the error message.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            CoreError::SpotifyForbidden { .. } => Some(
                "Check the README setup section: the app owner adds each user under User \
                 Management and needs an active Premium subscription.",
            ),
            CoreError::Network(_) => Some("Check your internet connection and try again."),
            CoreError::SessionExpired => Some("Sign in again from the sign-in screen."),
            CoreError::ReauthNeeded => Some(
                "Spotify will show the consent page again; approve it to continue. Nothing else changes.",
            ),
            CoreError::NoKeyring(_) => {
                Some("Start or unlock your keyring, then restart playlist-porter.")
            }
            CoreError::Config(_) => Some("Open Settings → Edit setup… to fix the app credentials."),
            CoreError::YouTubeQuotaExceeded => {
                Some("Use Resume transfer on the Playlists screen after the reset.")
            }
            _ => None,
        }
    }

    /// Whether the README setup link should be offered with this error.
    pub fn wants_setup_link(&self) -> bool {
        matches!(
            self,
            CoreError::SpotifyForbidden { .. } | CoreError::Config(_) | CoreError::Auth(_)
        )
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e.to_string())
    }
}

impl From<reqwest::Error> for CoreError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors may include the URL. Strip query strings so nothing
        // sensitive (codes, tokens) can end up in a message.
        CoreError::Network(e.without_url().to_string())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
