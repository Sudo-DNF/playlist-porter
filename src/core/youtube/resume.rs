//! Resume state for a paused YouTube transfer.
//!
//! `$XDG_STATE_HOME/playlist-porter/resume/<spotify_playlist_id>.json`, holding
//! only: Spotify playlist ID, YouTube playlist ID, index of the next track and
//! per-track results so far (YouTube video ID + status). No Spotify track
//! metadata. Deleted when the run finishes, on sign-out, and automatically
//! after 7 days (well under YouTube's 30-day limit).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::core::config::APP_NAME;
use crate::core::error::{CoreError, CoreResult};
use crate::core::matching::MatchStatus;
use crate::core::spotify::client::is_valid_id;

pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackResult {
    pub video_id: Option<String>,
    pub status: MatchStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeState {
    pub spotify_playlist_id: String,
    pub youtube_playlist_id: String,
    pub next_index: usize,
    pub results: Vec<TrackResult>,
}

/// `$XDG_STATE_HOME/playlist-porter/resume` (falls back to the local data
/// dir on platforms without a state dir).
pub fn default_dir() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|d| d.join(APP_NAME).join("resume"))
}

/// Resume files in one directory. `dir` is injectable for tests.
#[derive(Debug, Clone)]
pub struct ResumeStore {
    dir: PathBuf,
}

impl ResumeStore {
    pub fn new(dir: PathBuf) -> Self {
        ResumeStore { dir }
    }

    pub fn open_default() -> CoreResult<Self> {
        default_dir()
            .map(Self::new)
            .ok_or_else(|| CoreError::Io("no state directory on this system".into()))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, spotify_playlist_id: &str) -> CoreResult<PathBuf> {
        if !is_valid_id(spotify_playlist_id) {
            return Err(CoreError::Io("invalid playlist ID".into()));
        }
        Ok(self.dir.join(format!("{spotify_playlist_id}.json")))
    }

    pub fn save(&self, state: &ResumeState) -> CoreResult<()> {
        let path = self.path(&state.spotify_playlist_id)?;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("json.tmp");
        let body =
            serde_json::to_vec(state).map_err(|e| CoreError::Io(format!("resume state: {e}")))?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Load saved state, ignoring (and deleting) expired or unreadable files.
    pub fn load(
        &self,
        spotify_playlist_id: &str,
        now: SystemTime,
    ) -> CoreResult<Option<ResumeState>> {
        let path = self.path(spotify_playlist_id)?;
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if is_expired(meta.modified().ok(), now) {
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        match serde_json::from_slice::<ResumeState>(&bytes) {
            Ok(s) if s.spotify_playlist_id == spotify_playlist_id => Ok(Some(s)),
            _ => {
                let _ = std::fs::remove_file(&path);
                Ok(None)
            }
        }
    }

    pub fn delete(&self, spotify_playlist_id: &str) -> CoreResult<bool> {
        let path = self.path(spotify_playlist_id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete every resume and temp file (sign-out). Returns how many
    /// resume files were removed.
    pub fn delete_all(&self) -> CoreResult<usize> {
        let mut n = 0;
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with(".json") || name.ends_with(".json.tmp") {
                std::fs::remove_file(&p)?;
                if name.ends_with(".json") {
                    n += 1;
                }
            }
        }
        let _ = std::fs::remove_dir(&self.dir);
        Ok(n)
    }

    /// Remove expired files and return the playlist IDs that still have state.
    pub fn purge_and_list(&self, now: SystemTime) -> CoreResult<Vec<(String, usize)>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.ends_with(".json.tmp") {
                let _ = std::fs::remove_file(&p);
                continue;
            }
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if !is_valid_id(id) {
                continue;
            }
            if let Some(state) = self.load(id, now)? {
                out.push((id.to_string(), state.next_index));
            }
        }
        out.sort();
        Ok(out)
    }
}

/// Older than 7 days (or with an unknown mtime) ⇒ expired.
pub fn is_expired(modified: Option<SystemTime>, now: SystemTime) -> bool {
    match modified {
        Some(m) => now
            .duration_since(m)
            .map(|age| age > MAX_AGE)
            .unwrap_or(false),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> ResumeStore {
        ResumeStore::new(
            std::env::temp_dir().join(format!("pp-resume-{tag}-{}", std::process::id())),
        )
    }

    fn state(id: &str) -> ResumeState {
        ResumeState {
            spotify_playlist_id: id.into(),
            youtube_playlist_id: "PLyt".into(),
            next_index: 2,
            results: vec![
                TrackResult {
                    video_id: Some("vid1".into()),
                    status: MatchStatus::Matched,
                },
                TrackResult {
                    video_id: None,
                    status: MatchStatus::NotFound,
                },
            ],
        }
    }

    #[test]
    fn roundtrip_and_minimal_contents() {
        let s = store("rt");
        s.save(&state("abc123")).unwrap();
        let raw = std::fs::read_to_string(s.dir().join("abc123.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "next_index",
                "results",
                "spotify_playlist_id",
                "youtube_playlist_id"
            ]
        );
        assert_eq!(
            s.load("abc123", SystemTime::now()).unwrap(),
            Some(state("abc123"))
        );
        assert_eq!(
            s.purge_and_list(SystemTime::now()).unwrap(),
            vec![("abc123".into(), 2)]
        );
        assert_eq!(s.delete_all().unwrap(), 1);
        assert_eq!(s.load("abc123", SystemTime::now()).unwrap(), None);
    }

    #[test]
    fn expiry_after_seven_days() {
        let now = SystemTime::now();
        let day = Duration::from_secs(86_400);
        assert!(!is_expired(Some(now - 6 * day), now));
        assert!(is_expired(Some(now - 8 * day), now));
        assert!(is_expired(None, now));
        // Expired files are deleted on load/list.
        let s = store("exp");
        s.save(&state("old1")).unwrap();
        assert_eq!(s.load("old1", now + 8 * day).unwrap(), None);
        assert!(!s.dir().join("old1.json").exists());
        let _ = s.delete_all();
    }

    #[test]
    fn rejects_bad_ids() {
        let s = store("ids");
        let mut st = state("x");
        st.spotify_playlist_id = "../../etc/passwd".into();
        assert!(s.save(&st).is_err());
        assert!(s.load("../x", SystemTime::now()).is_err());
    }
}
