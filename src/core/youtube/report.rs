//! The match report: one row per track with its status, YouTube link (if
//! any) and Spotify link, plus counts per status. No other statistics.

use serde::Serialize;

use crate::core::config::ExportFormat;
use crate::core::error::{CoreError, CoreResult};
use crate::core::export::{csv_safe, ATTRIBUTION};
use crate::core::matching::{fmt_diff, MatchStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportRow {
    /// 1-based position among the transferred tracks.
    pub position: usize,
    pub title: String,
    pub artists: String,
    pub spotify_url: String,
    pub status: MatchStatus,
    pub video_id: Option<String>,
    /// Unknown for rows restored from resume state (only IDs are saved).
    pub video_title: Option<String>,
    pub channel: Option<String>,
    /// Candidate minus Spotify, seconds.
    pub duration_diff_secs: Option<i64>,
    pub note: Option<String>,
}

impl ReportRow {
    pub fn youtube_url(&self) -> Option<String> {
        self.video_id
            .as_ref()
            .map(|id| format!("https://www.youtube.com/watch?v={id}"))
    }
}

/// Counts per status, shown on the filter tabs and in the summary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub matched: usize,
    pub uncertain: usize,
    pub not_found: usize,
}

impl Counts {
    pub fn of(rows: &[ReportRow]) -> Self {
        let mut c = Counts::default();
        for r in rows {
            match r.status {
                MatchStatus::Matched => c.matched += 1,
                MatchStatus::Uncertain => c.uncertain += 1,
                MatchStatus::NotFound => c.not_found += 1,
            }
        }
        c
    }

    pub fn total(&self) -> usize {
        self.matched + self.uncertain + self.not_found
    }
}

pub fn youtube_playlist_url(id: &str) -> String {
    format!("https://www.youtube.com/playlist?list={id}")
}

pub fn to_csv(
    playlist_url: &str,
    youtube_playlist_id: Option<&str>,
    rows: &[ReportRow],
) -> CoreResult<String> {
    let mut out = format!("# {ATTRIBUTION} Playlist: {playlist_url}\n");
    if let Some(id) = youtube_playlist_id {
        out.push_str(&format!(
            "# YouTube playlist: {}\n",
            youtube_playlist_url(id)
        ));
    }
    let c = Counts::of(rows);
    out.push_str(&format!(
        "# Matched: {}, Uncertain: {}, Not found: {}\n",
        c.matched, c.uncertain, c.not_found
    ));
    let mut w = csv::Writer::from_writer(Vec::new());
    let err = |e: csv::Error| CoreError::Io(e.to_string());
    w.write_record([
        "position",
        "status",
        "title",
        "artists",
        "spotify_url",
        "youtube_url",
        "youtube_title",
        "youtube_channel",
        "duration_diff",
        "note",
    ])
    .map_err(err)?;
    for r in rows {
        w.write_record([
            r.position.to_string(),
            r.status.label().to_string(),
            csv_safe(&r.title),
            csv_safe(&r.artists),
            r.spotify_url.clone(),
            r.youtube_url().unwrap_or_default(),
            csv_safe(r.video_title.as_deref().unwrap_or("")),
            csv_safe(r.channel.as_deref().unwrap_or("")),
            if r.duration_diff_secs.is_some() {
                fmt_diff(r.duration_diff_secs)
            } else {
                String::new()
            },
            r.note.clone().unwrap_or_default(),
        ])
        .map_err(err)?;
    }
    let bytes = w.into_inner().map_err(|e| CoreError::Io(e.to_string()))?;
    out.push_str(&String::from_utf8_lossy(&bytes));
    Ok(out)
}

#[derive(Serialize)]
struct JsonRow<'a> {
    #[serde(flatten)]
    row: &'a ReportRow,
    youtube_url: Option<String>,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    attribution: &'a str,
    playlist_url: &'a str,
    youtube_playlist_url: Option<String>,
    counts: JsonCounts,
    tracks: Vec<JsonRow<'a>>,
}

#[derive(Serialize)]
struct JsonCounts {
    matched: usize,
    uncertain: usize,
    not_found: usize,
}

pub fn to_json(
    playlist_url: &str,
    youtube_playlist_id: Option<&str>,
    rows: &[ReportRow],
) -> CoreResult<String> {
    let c = Counts::of(rows);
    let doc = JsonReport {
        attribution: ATTRIBUTION,
        playlist_url,
        youtube_playlist_url: youtube_playlist_id.map(youtube_playlist_url),
        counts: JsonCounts {
            matched: c.matched,
            uncertain: c.uncertain,
            not_found: c.not_found,
        },
        tracks: rows
            .iter()
            .map(|r| JsonRow {
                row: r,
                youtube_url: r.youtube_url(),
            })
            .collect(),
    };
    serde_json::to_string_pretty(&doc).map_err(|e| CoreError::Io(e.to_string()))
}

pub fn render(
    format: ExportFormat,
    playlist_url: &str,
    youtube_playlist_id: Option<&str>,
    rows: &[ReportRow],
) -> CoreResult<String> {
    match format {
        ExportFormat::Csv => to_csv(playlist_url, youtube_playlist_id, rows),
        ExportFormat::Json => to_json(playlist_url, youtube_playlist_id, rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<ReportRow> {
        vec![
            ReportRow {
                position: 1,
                title: "Song".into(),
                artists: "Artist".into(),
                spotify_url: "https://open.spotify.com/track/a".into(),
                status: MatchStatus::Matched,
                video_id: Some("vid".into()),
                video_title: Some("Artist - Song".into()),
                channel: Some("Artist - Topic".into()),
                duration_diff_secs: Some(1),
                note: None,
            },
            ReportRow {
                position: 2,
                title: "=Other".into(),
                artists: "B".into(),
                spotify_url: "https://open.spotify.com/track/b".into(),
                status: MatchStatus::NotFound,
                video_id: None,
                video_title: None,
                channel: None,
                duration_diff_secs: None,
                note: Some("No usable result".into()),
            },
        ]
    }

    #[test]
    fn csv_report_has_attribution_and_both_links() {
        let csv = to_csv("https://open.spotify.com/playlist/p", Some("PL1"), &rows()).unwrap();
        let first = csv.lines().next().unwrap();
        assert!(
            first.contains(ATTRIBUTION) && first.contains("https://open.spotify.com/playlist/p")
        );
        assert!(csv.contains("https://www.youtube.com/playlist?list=PL1"));
        assert!(csv.contains("https://open.spotify.com/track/a"));
        assert!(csv.contains("https://open.spotify.com/track/b"));
        assert!(csv.contains("https://www.youtube.com/watch?v=vid"));
        assert!(csv.contains("'=Other"));
        assert!(csv.contains("# Matched: 1, Uncertain: 0, Not found: 1"));
    }

    #[test]
    fn json_report_has_attribution_and_per_track_spotify_url() {
        let v: serde_json::Value = serde_json::from_str(
            &to_json("https://open.spotify.com/playlist/p", None, &rows()).unwrap(),
        )
        .unwrap();
        assert_eq!(v["attribution"], ATTRIBUTION);
        assert_eq!(v["playlist_url"], "https://open.spotify.com/playlist/p");
        assert_eq!(
            v["tracks"][0]["spotify_url"],
            "https://open.spotify.com/track/a"
        );
        assert_eq!(
            v["tracks"][0]["youtube_url"],
            "https://www.youtube.com/watch?v=vid"
        );
        assert_eq!(
            v["tracks"][1]["spotify_url"],
            "https://open.spotify.com/track/b"
        );
        assert_eq!(v["counts"]["not_found"], 1);
    }
}
