//! Serde models for the four Spotify Web API endpoints we use, in their
//! post-February/March 2026 shape.
//!
//! Renamed fields: playlist `tracks` → `items`, and playlist-item `track` →
//! `item`. The API may currently send old names, new names or *both*. A plain
//! `#[serde(alias = ...)]` errors with "duplicate field" when both are present,
//! so each pair is modelled as two optional fields and the accessor prefers
//! the new name. Responses with only one name still work exactly like an alias.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

impl User {
    pub fn name(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct ExternalUrls {
    #[serde(default)]
    pub spotify: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Paging<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub total: Option<u32>,
}

/// The `items` (formerly `tracks`) field of a playlist object: `{href, total}`
/// in list responses, a paging object in the full playlist. We only need
/// `total`; everything else is ignored.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ItemsSummary {
    #[serde(default)]
    pub total: Option<u32>,
}

/// A playlist as returned by `GET /me/playlists` and `GET /playlists/{id}`.
#[derive(Debug, Clone, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner: User,
    #[serde(default)]
    pub collaborative: bool,
    #[serde(default)]
    pub external_urls: ExternalUrls,
    /// New name. Absent for playlists the user doesn't own or collaborate on.
    #[serde(default)]
    items: Option<ItemsSummary>,
    /// Old name, still sent by some responses.
    #[serde(default, rename = "tracks")]
    tracks_legacy: Option<ItemsSummary>,
}

/// Pseudo playlist ID for the user's Liked Songs (saved tracks). Real
/// playlist IDs are 22 characters, so it can't collide.
pub const LIKED_SONGS_ID: &str = "liked";
pub const LIKED_SONGS_NAME: &str = "Liked Songs";
/// Attribution link for Liked Songs: the user's library page.
pub const LIKED_SONGS_URL: &str = "https://open.spotify.com/collection/tracks";

impl Playlist {
    /// The user's Liked Songs, presented as a playlist they own.
    pub fn liked_songs(me: &User, total: Option<u32>) -> Playlist {
        Playlist {
            id: LIKED_SONGS_ID.into(),
            name: LIKED_SONGS_NAME.into(),
            owner: me.clone(),
            collaborative: false,
            external_urls: ExternalUrls {
                spotify: Some(LIKED_SONGS_URL.into()),
            },
            items: Some(ItemsSummary { total }),
            tracks_legacy: None,
        }
    }

    pub fn is_liked_songs(&self) -> bool {
        self.id == LIKED_SONGS_ID
    }

    /// Item summary, preferring the new `items` field.
    pub fn items(&self) -> Option<&ItemsSummary> {
        self.items.as_ref().or(self.tracks_legacy.as_ref())
    }

    pub fn total(&self) -> Option<u32> {
        self.items().and_then(|i| i.total)
    }

    pub fn spotify_url(&self) -> String {
        self.external_urls
            .spotify
            .clone()
            .unwrap_or_else(|| format!("https://open.spotify.com/playlist/{}", self.id))
    }
}

/// One entry of `GET /playlists/{id}/items`.
#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistEntry {
    #[serde(default)]
    pub is_local: bool,
    /// New name. May be `null` (removed/unavailable).
    #[serde(default)]
    item: Option<Playable>,
    /// Old name.
    #[serde(default, rename = "track")]
    track_legacy: Option<Playable>,
}

impl PlaylistEntry {
    pub fn playable(&self) -> Option<&Playable> {
        self.item.as_ref().or(self.track_legacy.as_ref())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Playable {
    Track(Track),
    Episode(Episode),
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct ExternalIds {
    /// Removed and later restored by Spotify: always optional.
    #[serde(default)]
    pub isrc: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistRef {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlbumRef {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Track {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub artists: Vec<ArtistRef>,
    #[serde(default)]
    pub album: Option<AlbumRef>,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub external_ids: Option<ExternalIds>,
    #[serde(default)]
    pub external_urls: Option<ExternalUrls>,
    #[serde(default)]
    pub is_local: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Episode {
    #[serde(default)]
    pub name: String,
}

/// Normalised view of one playlist entry: what the UI, export and transfer
/// work with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Track(TrackInfo),
    /// A local file: no Spotify ID or URL. Skipped and reported.
    Local {
        name: String,
        duration_ms: u64,
    },
    /// A podcast episode. Skipped and reported.
    Episode {
        name: String,
    },
    /// `null` item or a track without an ID/URL. Skipped and reported.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration_ms: u64,
    pub isrc: Option<String>,
    pub spotify_url: String,
}

impl TrackInfo {
    pub fn primary_artist(&self) -> &str {
        self.artists.first().map(String::as_str).unwrap_or("")
    }

    pub fn artists_joined(&self) -> String {
        self.artists.join(", ")
    }
}

impl Entry {
    pub fn from_api(e: &PlaylistEntry) -> Entry {
        match e.playable() {
            None => Entry::Unavailable,
            Some(Playable::Episode(ep)) => Entry::Episode {
                name: ep.name.clone(),
            },
            Some(Playable::Other) => Entry::Unavailable,
            Some(Playable::Track(t)) => {
                if e.is_local || t.is_local {
                    return Entry::Local {
                        name: t.name.clone(),
                        duration_ms: t.duration_ms,
                    };
                }
                let url = t.external_urls.as_ref().and_then(|u| u.spotify.clone());
                match (&t.id, url) {
                    (Some(id), Some(url)) if !id.is_empty() => Entry::Track(TrackInfo {
                        id: id.clone(),
                        title: t.name.clone(),
                        artists: t.artists.iter().map(|a| a.name.clone()).collect(),
                        album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
                        duration_ms: t.duration_ms,
                        isrc: t
                            .external_ids
                            .as_ref()
                            .and_then(|x| x.isrc.clone())
                            .filter(|s| !s.is_empty()),
                        spotify_url: url,
                    }),
                    _ => Entry::Unavailable,
                }
            }
        }
    }

    pub fn track(&self) -> Option<&TrackInfo> {
        match self {
            Entry::Track(t) => Some(t),
            _ => None,
        }
    }
}

/// Counts of entries that exports and transfers skip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkipCounts {
    pub local: usize,
    pub episode: usize,
    pub unavailable: usize,
}

impl SkipCounts {
    pub fn of(entries: &[Entry]) -> Self {
        let mut c = SkipCounts::default();
        for e in entries {
            match e {
                Entry::Local { .. } => c.local += 1,
                Entry::Episode { .. } => c.episode += 1,
                Entry::Unavailable => c.unavailable += 1,
                Entry::Track(_) => {}
            }
        }
        c
    }

    pub fn total(&self) -> usize {
        self.local + self.episode + self.unavailable
    }

    /// e.g. "1 local file, 1 unavailable track".
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        if self.local > 0 {
            parts.push(plural(self.local, "local file", "local files"));
        }
        if self.episode > 0 {
            parts.push(plural(self.episode, "podcast episode", "podcast episodes"));
        }
        if self.unavailable > 0 {
            parts.push(plural(
                self.unavailable,
                "unavailable track",
                "unavailable tracks",
            ));
        }
        parts.join(", ")
    }
}

/// Spotify's error body: `{"error": {"status": 403, "message": "..."}}`.
#[derive(Debug, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiError,
}

#[derive(Debug, Deserialize)]
pub struct ApiError {
    #[serde(default)]
    pub message: String,
}

/// Extract `error.message`, or an empty string.
pub fn api_error_message(body: &str) -> String {
    serde_json::from_str::<ApiErrorBody>(body)
        .map(|b| b.error.message)
        .unwrap_or_default()
}
