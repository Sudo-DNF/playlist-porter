//! Token storage in the OS keyring, and nowhere else.
//!
//! Service `playlist-porter`, entries `spotify` and `google`. If no keyring is
//! available we return a clear error; there is no plaintext fallback.

use super::StoredToken;
use crate::core::error::{CoreError, CoreResult};

pub const SERVICE: &str = "playlist-porter";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Account {
    Spotify,
    Google,
}

impl Account {
    pub fn entry_name(self) -> &'static str {
        match self {
            Account::Spotify => "spotify",
            Account::Google => "google",
        }
    }
}

fn map_err(e: keyring::Error) -> CoreError {
    // Never format variants that carry stored bytes.
    match e {
        keyring::Error::NoDefaultStore => {
            CoreError::NoKeyring("no credential store could be opened".into())
        }
        keyring::Error::PlatformFailure(p) | keyring::Error::NoStorageAccess(p) => {
            CoreError::NoKeyring(p.to_string())
        }
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(_, _) => {
            CoreError::Keyring("the stored token is unreadable; sign in again".into())
        }
        other => CoreError::Keyring(format!("{other:?}").chars().take(120).collect()),
    }
}

fn entry(account: Account) -> CoreResult<keyring::Entry> {
    keyring::Entry::new(SERVICE, account.entry_name()).map_err(map_err)
}

fn load_blocking(account: Account) -> CoreResult<Option<StoredToken>> {
    let e = entry(account)?;
    match e.get_password() {
        Ok(s) => match serde_json::from_str::<StoredToken>(&s) {
            Ok(t) => Ok(Some(t)),
            // Unreadable entry: treat as signed out and remove it.
            Err(_) => {
                let _ = e.delete_credential();
                Ok(None)
            }
        },
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(map_err(err)),
    }
}

fn save_blocking(account: Account, token: &StoredToken) -> CoreResult<()> {
    let body = serde_json::to_string(token)
        .map_err(|_| CoreError::Keyring("couldn't encode token".into()))?;
    entry(account)?.set_password(&body).map_err(map_err)
}

/// Returns true if an entry existed and was deleted.
fn delete_blocking(account: Account) -> CoreResult<bool> {
    match entry(account)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(map_err(e)),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| CoreError::Keyring(e.to_string()))?
}

pub async fn load(account: Account) -> CoreResult<Option<StoredToken>> {
    blocking(move || load_blocking(account)).await
}

pub async fn save(account: Account, token: StoredToken) -> CoreResult<()> {
    blocking(move || save_blocking(account, &token)).await
}

pub async fn delete(account: Account) -> CoreResult<bool> {
    blocking(move || delete_blocking(account)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_match_spec() {
        assert_eq!(SERVICE, "playlist-porter");
        assert_eq!(Account::Spotify.entry_name(), "spotify");
        assert_eq!(Account::Google.entry_name(), "google");
    }

    #[test]
    fn token_debug_is_redacted() {
        let t = StoredToken {
            access_token: "SECRET-A".into(),
            refresh_token: Some("SECRET-R".into()),
            expires_at: 5,
            scope: None,
        };
        let s = format!("{t:?}");
        assert!(!s.contains("SECRET"));
    }
}
