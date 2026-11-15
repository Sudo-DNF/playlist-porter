//! A signed-in session: holds the current token in memory, refreshes it
//! before expiry or after a 401, and persists refreshed tokens to the keyring.

use std::future::Future;
use std::pin::Pin;

use tokio::sync::Mutex;

use super::keyring::{self, Account};
use super::{now_unix, StoredToken};
use crate::core::error::{CoreError, CoreResult};

pub type RefreshFuture = Pin<Box<dyn Future<Output = CoreResult<StoredToken>> + Send>>;
pub type RefreshFn = Box<dyn Fn(String) -> RefreshFuture + Send + Sync>;

pub struct Session {
    account: Account,
    token: Mutex<StoredToken>,
    refresher: RefreshFn,
    /// Disable keyring writes (unit tests only).
    persist: bool,
}

impl Session {
    pub fn new(account: Account, token: StoredToken, refresher: RefreshFn) -> Self {
        Session {
            account,
            token: Mutex::new(token),
            refresher,
            persist: true,
        }
    }

    #[cfg(test)]
    pub fn new_ephemeral(account: Account, token: StoredToken, refresher: RefreshFn) -> Self {
        Session {
            persist: false,
            ..Session::new(account, token, refresher)
        }
    }

    pub fn account(&self) -> Account {
        self.account
    }

    /// A current access token, refreshing first if it is about to expire.
    pub async fn access_token(&self) -> CoreResult<String> {
        let mut t = self.token.lock().await;
        if t.needs_refresh(now_unix()) {
            *t = self.do_refresh(&t).await?;
        }
        Ok(t.access_token.clone())
    }

    /// Refresh after a 401. `stale` is the access token that was rejected; if
    /// another request already refreshed it, the new one is returned as is.
    pub async fn refresh_after_401(&self, stale: &str) -> CoreResult<String> {
        let mut t = self.token.lock().await;
        if t.access_token != stale {
            return Ok(t.access_token.clone());
        }
        *t = self.do_refresh(&t).await?;
        Ok(t.access_token.clone())
    }

    pub async fn snapshot(&self) -> StoredToken {
        self.token.lock().await.clone()
    }

    async fn do_refresh(&self, current: &StoredToken) -> CoreResult<StoredToken> {
        let Some(rt) = current.refresh_token.clone() else {
            return Err(CoreError::SessionExpired);
        };
        let new = match (self.refresher)(rt).await {
            // A refresh response may omit `scope`; the grant is unchanged.
            Ok(mut t) => {
                if t.scope.is_none() {
                    t.scope = current.scope.clone();
                }
                t
            }
            // Network trouble isn't an expired session; surface it as is.
            Err(e @ CoreError::Network(_)) => return Err(e),
            Err(_) => return Err(CoreError::SessionExpired),
        };
        if self.persist {
            keyring::save(self.account, new.clone()).await?;
        }
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn tok(a: &str, exp: i64) -> StoredToken {
        StoredToken {
            access_token: a.into(),
            refresh_token: Some("r".into()),
            expires_at: exp,
            scope: None,
        }
    }

    #[tokio::test]
    async fn refreshes_when_expiring_and_once_per_401() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let s = Session::new_ephemeral(
            Account::Spotify,
            tok("old", 0),
            Box::new(move |_rt| {
                let n = c.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(tok(&format!("new{n}"), now_unix() + 3600)) })
            }),
        );
        assert_eq!(s.access_token().await.unwrap(), "new0");
        assert_eq!(s.access_token().await.unwrap(), "new0");
        // A 401 with a token that was already replaced doesn't refresh again.
        assert_eq!(s.refresh_after_401("old").await.unwrap(), "new0");
        assert_eq!(s.refresh_after_401("new0").await.unwrap(), "new1");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_refresh_means_session_expired() {
        let s = Session::new_ephemeral(
            Account::Spotify,
            tok("old", 0),
            Box::new(|_| Box::pin(async { Err(CoreError::Auth("invalid_grant".into())) })),
        );
        assert_eq!(s.access_token().await, Err(CoreError::SessionExpired));
    }
}
