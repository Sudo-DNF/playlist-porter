//! CSV/JSON export of one playlist the user owns or collaborates on.
//!
//! Attribution (Policy II.4): the CSV starts with a `#` comment crediting
//! Spotify and giving the playlist URL; JSON has top-level `attribution` and
//! `playlist_url`. Every track carries its `spotify_url`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::config::ExportFormat;
use super::error::{CoreError, CoreResult};
use super::spotify::client::ensure_exportable;
use super::spotify::models::{Entry, Playlist, SkipCounts};

pub const ATTRIBUTION: &str = "Playlist and track data supplied by Spotify.";

/// What an export needs to know about the playlist.
#[derive(Debug, Clone)]
pub struct PlaylistMeta {
    pub name: String,
    pub url: String,
}

impl PlaylistMeta {
    pub fn from_playlist(p: &Playlist) -> Self {
        PlaylistMeta {
            name: p.name.clone(),
            url: p.spotify_url(),
        }
    }
}

#[derive(Debug, Serialize)]
struct JsonTrack<'a> {
    position: usize,
    title: &'a str,
    artists: &'a [String],
    album: &'a str,
    duration_ms: u64,
    isrc: Option<&'a str>,
    spotify_url: &'a str,
}

#[derive(Debug, Serialize)]
struct JsonSkipped {
    local_files: usize,
    episodes: usize,
    unavailable: usize,
}

#[derive(Debug, Serialize)]
struct JsonExport<'a> {
    attribution: &'a str,
    playlist_name: &'a str,
    playlist_url: &'a str,
    track_count: usize,
    tracks: Vec<JsonTrack<'a>>,
    skipped: JsonSkipped,
}

/// Strip line breaks and control characters (for the CSV comment line).
fn one_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Neutralise spreadsheet formula injection: a cell starting with `=`, `+`,
/// `-`, `@`, tab or CR gets a leading apostrophe (OWASP recommendation).
pub fn csv_safe(s: &str) -> String {
    match s.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{s}"),
        _ => s.to_string(),
    }
}

pub fn to_csv(meta: &PlaylistMeta, entries: &[Entry]) -> CoreResult<String> {
    let mut out = format!(
        "# {ATTRIBUTION} Playlist \"{}\": {}\n",
        one_line(&meta.name).replace('"', "'"),
        one_line(&meta.url)
    );
    let mut w = csv::Writer::from_writer(Vec::new());
    let err = |e: csv::Error| CoreError::Io(e.to_string());
    w.write_record([
        "position",
        "title",
        "artists",
        "album",
        "duration_ms",
        "isrc",
        "spotify_url",
    ])
    .map_err(err)?;
    for (i, e) in entries.iter().enumerate() {
        let Some(t) = e.track() else { continue };
        w.write_record([
            (i + 1).to_string(),
            csv_safe(&t.title),
            csv_safe(&t.artists.join("; ")),
            csv_safe(&t.album),
            t.duration_ms.to_string(),
            t.isrc.clone().unwrap_or_default(),
            t.spotify_url.clone(),
        ])
        .map_err(err)?;
    }
    let bytes = w.into_inner().map_err(|e| CoreError::Io(e.to_string()))?;
    out.push_str(&String::from_utf8_lossy(&bytes));
    Ok(out)
}

pub fn to_json(meta: &PlaylistMeta, entries: &[Entry]) -> CoreResult<String> {
    let tracks: Vec<JsonTrack<'_>> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            e.track().map(|t| JsonTrack {
                position: i + 1,
                title: &t.title,
                artists: &t.artists,
                album: &t.album,
                duration_ms: t.duration_ms,
                isrc: t.isrc.as_deref(),
                spotify_url: &t.spotify_url,
            })
        })
        .collect();
    let skipped = SkipCounts::of(entries);
    let doc = JsonExport {
        attribution: ATTRIBUTION,
        playlist_name: &meta.name,
        playlist_url: &meta.url,
        track_count: tracks.len(),
        tracks,
        skipped: JsonSkipped {
            local_files: skipped.local,
            episodes: skipped.episode,
            unavailable: skipped.unavailable,
        },
    };
    serde_json::to_string_pretty(&doc).map_err(|e| CoreError::Io(e.to_string()))
}

pub fn render(format: ExportFormat, meta: &PlaylistMeta, entries: &[Entry]) -> CoreResult<String> {
    match format {
        ExportFormat::Csv => to_csv(meta, entries),
        ExportFormat::Json => to_json(meta, entries),
    }
}

/// Write `body` to `path` atomically. Refuses to replace an existing file
/// unless `overwrite` is set (the UI asks first).
pub fn write_file(path: &Path, body: &str, overwrite: bool) -> CoreResult<()> {
    if path.as_os_str().is_empty() {
        return Err(CoreError::Io("choose where to save the file".into()));
    }
    if path.is_dir() {
        return Err(CoreError::Io(format!("{} is a folder", path.display())));
    }
    if !overwrite && path.exists() {
        return Err(CoreError::FileExists(path.display().to_string()));
    }
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !dir.is_dir() {
        return Err(CoreError::Io(format!(
            "the folder {} doesn't exist",
            dir.display()
        )));
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".into());
    let tmp = dir.join(format!(".{file_name}.partial"));
    std::fs::write(&tmp, body)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Export a playlist. Re-checks ownership so the UI can't bypass it.
/// Returns the number of tracks written.
pub fn export_playlist(
    playlist: &Playlist,
    entries: &[Entry],
    me_id: &str,
    format: ExportFormat,
    path: &Path,
    overwrite: bool,
) -> CoreResult<usize> {
    ensure_exportable(playlist, me_id)?;
    let meta = PlaylistMeta::from_playlist(playlist);
    let body = render(format, &meta, entries)?;
    write_file(path, &body, overwrite)?;
    Ok(entries.iter().filter(|e| e.track().is_some()).count())
}

/// A safe file name from a playlist name.
pub fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let mut out: String = trimmed.chars().take(100).collect();
    if out.is_empty() {
        out = "playlist".into();
    }
    out
}

/// Default save path: last export folder, else Documents, else home.
pub fn default_path(name: &str, format: ExportFormat, last_dir: Option<&Path>) -> PathBuf {
    let dir = last_dir
        .filter(|d| d.is_dir())
        .map(Path::to_path_buf)
        .or_else(dirs::document_dir)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join(format!(
        "{}.{}",
        sanitize_file_name(name),
        format.extension()
    ))
}

/// Expand a leading `~/` typed by the user.
pub fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(sanitize_file_name("Road Trip 2026"), "Road Trip 2026");
        assert_eq!(sanitize_file_name("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_file_name(".."), "playlist");
        assert_eq!(sanitize_file_name("   "), "playlist");
    }

    #[test]
    fn formula_injection_is_neutralised() {
        assert_eq!(csv_safe("=HYPERLINK(\"x\")"), "'=HYPERLINK(\"x\")");
        assert_eq!(csv_safe("@SUM"), "'@SUM");
        assert_eq!(csv_safe("Normal"), "Normal");
    }

    #[test]
    fn overwrite_needs_consent() {
        let dir = std::env::temp_dir().join(format!("pp-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.csv");
        write_file(&p, "one", false).unwrap();
        assert_eq!(
            write_file(&p, "two", false),
            Err(CoreError::FileExists(p.display().to_string()))
        );
        write_file(&p, "two", true).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "two");
        // No temp file is left behind.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
