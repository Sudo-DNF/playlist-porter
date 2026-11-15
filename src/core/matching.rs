//! Track ↔ YouTube video matching. Pure and deterministic: string and
//! duration logic only, no I/O, no ML. Every rule here is unit-tested.
//!
//! Flow: build a query from `"<primary artist> <title>"` with remaster/feat.
//! suffixes stripped, fetch candidates with `search.list` and their durations
//! with `videos.list`, then [`choose`] the best candidate and classify it.

use std::collections::HashSet;

/// Best score at or above this *and* within ±5 s ⇒ Matched.
pub const HIGH_THRESHOLD: f32 = 0.85;
/// Below this (or with too little title overlap) ⇒ Not found.
pub const MIN_USABLE: f32 = 0.45;
/// Minimum title similarity for a candidate to be usable at all.
pub const MIN_TITLE: f32 = 0.34;
/// Duration difference that counts as "the same recording".
pub const DURATION_TOLERANCE_SECS: u64 = 5;

/// Words that mark a different version; rejected unless the Spotify title
/// has them too.
pub const EXCLUDED_WORDS: [&str; 3] = ["live", "cover", "remix"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MatchStatus {
    Matched,
    Uncertain,
    NotFound,
}

impl MatchStatus {
    pub fn label(self) -> &'static str {
        match self {
            MatchStatus::Matched => "Matched",
            MatchStatus::Uncertain => "Uncertain",
            MatchStatus::NotFound => "Not found",
        }
    }

    /// Whether the video is added to the YouTube playlist.
    pub fn is_added(self) -> bool {
        !matches!(self, MatchStatus::NotFound)
    }
}

/// The Spotify side of a match.
#[derive(Debug, Clone)]
pub struct TrackQuery<'a> {
    pub title: &'a str,
    pub artists: &'a [String],
    pub duration_ms: u64,
}

impl TrackQuery<'_> {
    fn duration_secs(&self) -> u64 {
        (self.duration_ms + 500) / 1000
    }
}

/// A YouTube search result plus its duration from `videos.list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub duration_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub candidate: Candidate,
    pub score: f32,
    pub title_similarity: f32,
    /// Candidate minus Spotify, in seconds.
    pub duration_diff_secs: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchOutcome {
    pub status: MatchStatus,
    pub best: Option<Scored>,
    /// Short reason shown in the report ("Duration far off", …).
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Text normalisation

/// Fold common Latin diacritics to ASCII (é → e, ø → o, ß → ss).
fn fold_char(c: char, out: &mut String) {
    let s = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ğ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
        'ł' | 'ľ' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'œ' => "oe",
        'ř' => "r",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => {
            out.push(c);
            return;
        }
    };
    out.push_str(s);
}

/// Lowercase, fold diacritics, `&` → `and`, punctuation → spaces, collapse.
pub fn normalize(s: &str) -> String {
    let mut folded = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        fold_char(c, &mut folded);
    }
    let mut out = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c == '&' {
            out.push_str(" and ");
        } else if c == '\'' || c == '’' {
            // "don't" → "dont"
        } else if c.is_alphanumeric() {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn tokens(s: &str) -> Vec<String> {
    normalize(s)
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whole-word, case-insensitive containment.
pub fn contains_word(haystack: &str, word: &str) -> bool {
    let w = normalize(word);
    tokens(haystack).contains(&w)
}

/// A candidate is rejected if its title has live/cover/remix as a whole
/// word and the Spotify title doesn't.
pub fn is_excluded(candidate_title: &str, spotify_title: &str) -> bool {
    EXCLUDED_WORDS
        .iter()
        .any(|w| contains_word(candidate_title, w) && !contains_word(spotify_title, w))
}

// ---------------------------------------------------------------------------
// Query building

/// Markers of a suffix that describes a release, not the song.
const SUFFIX_MARKERS: [&str; 14] = [
    "remaster",
    "remastered",
    "feat",
    "ft",
    "featuring",
    "radio edit",
    "single version",
    "album version",
    "mono",
    "stereo",
    "deluxe",
    "bonus track",
    "anniversary",
    "edition",
];

fn is_release_suffix(segment: &str) -> bool {
    let n = normalize(segment);
    if n.is_empty() {
        return true;
    }
    // A bare year, e.g. "- 2011".
    if n.len() == 4 && n.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let padded = format!(" {n} ");
    SUFFIX_MARKERS
        .iter()
        .any(|m| padded.contains(&format!(" {m} ")))
        || n.starts_with("with ")
}

/// Strip `- Remastered 2011`, `(feat. X)`, `[Radio Edit]` and similar from a
/// title, for the search query only.
pub fn strip_suffixes(title: &str) -> String {
    // Bracketed segments.
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(start) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[start] == b'(' {
            ')'
        } else {
            ']'
        };
        match rest[start + 1..].find(close) {
            Some(rel_end) => {
                let inner = &rest[start + 1..start + 1 + rel_end];
                out.push_str(&rest[..start]);
                if !is_release_suffix(inner) {
                    out.push_str(&rest[start..start + 1 + rel_end + 1]);
                }
                rest = &rest[start + 1 + rel_end + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    // " - " segments.
    let mut parts: Vec<&str> = out.split(" - ").collect();
    while parts.len() > 1 && is_release_suffix(parts[parts.len() - 1]) {
        parts.pop();
    }
    // An unbracketed "feat. X" tail.
    let mut joined = parts.join(" - ");
    for marker in [" feat. ", " ft. ", " featuring "] {
        if let Some(i) = joined.to_lowercase().find(marker) {
            joined.truncate(i);
        }
    }
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `"<primary artist> <title>"` with release suffixes stripped.
pub fn build_query(primary_artist: &str, title: &str) -> String {
    let t = strip_suffixes(title);
    let t = if t.is_empty() {
        title.trim().to_string()
    } else {
        t
    };
    format!("{} {}", primary_artist.trim(), t)
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Durations

/// Parse an ISO-8601 duration as YouTube sends it (`PT4M13S`, `PT1H2M`,
/// `P1DT2H`, `P0D`). Returns whole seconds, or `None` if malformed.
pub fn parse_iso8601_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    let rest = s.strip_prefix('P')?;
    if rest.is_empty() {
        return None;
    }
    let (date, time) = match rest.split_once('T') {
        Some((d, t)) => {
            if t.is_empty() {
                return None;
            }
            (d, Some(t))
        }
        None => (rest, None),
    };
    let mut total: u64 = 0;
    let mut num = String::new();
    for c in date.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            let n: u64 = num.parse().ok()?;
            num.clear();
            total += match c {
                'W' => n * 7 * 86_400,
                'D' => n * 86_400,
                _ => return None, // years/months aren't used for videos
            };
        }
    }
    if !num.is_empty() {
        return None;
    }
    if let Some(time) = time {
        for c in time.chars() {
            if c.is_ascii_digit() || c == '.' {
                num.push(c);
            } else {
                let n: f64 = num.parse().ok()?;
                num.clear();
                total += match c {
                    'H' => (n * 3600.0) as u64,
                    'M' => (n * 60.0) as u64,
                    'S' => n.round() as u64,
                    _ => return None,
                };
            }
        }
        if !num.is_empty() {
            return None;
        }
    }
    Some(total)
}

// ---------------------------------------------------------------------------
// Scoring

/// Tokens in video titles that say nothing about the song.
const NOISE: [&str; 17] = [
    "official",
    "video",
    "audio",
    "lyric",
    "lyrics",
    "hd",
    "hq",
    "4k",
    "music",
    "mv",
    "visualizer",
    "visualiser",
    "topic",
    "vevo",
    "clip",
    "remastered",
    "remaster",
];

/// How well the candidate title matches the Spotify title, 0..=1.
/// Recall of the (suffix-stripped) Spotify title words counts most; extra
/// words in the video title cost a little. Remaster/feat. words from the
/// full title give a small bonus when they appear too.
pub fn title_similarity(spotify_title: &str, candidate_title: &str, artists: &[String]) -> f32 {
    let core: HashSet<String> = tokens(&strip_suffixes(spotify_title)).into_iter().collect();
    let core = if core.is_empty() {
        tokens(spotify_title).into_iter().collect()
    } else {
        core
    };
    if core.is_empty() {
        return 0.0;
    }
    let artist_tokens: HashSet<String> = artists.iter().flat_map(|a| tokens(a)).collect();
    let cand: HashSet<String> = tokens(candidate_title)
        .into_iter()
        .filter(|t| !NOISE.contains(&t.as_str()))
        .filter(|t| !artist_tokens.contains(t) || core.contains(t))
        .collect();
    if cand.is_empty() {
        return 0.0;
    }
    let hit = core.intersection(&cand).count() as f32;
    let recall = hit / core.len() as f32;
    let full: HashSet<String> = tokens(spotify_title).into_iter().collect();
    let extra_ok = cand.iter().filter(|t| full.contains(*t)).count() as f32;
    let precision = (extra_ok / cand.len() as f32).min(1.0);
    (0.75 * recall + 0.25 * precision).clamp(0.0, 1.0)
}

fn compact(s: &str) -> String {
    normalize(s).replace(' ', "")
}

/// Artist presence in the title or channel name, 0..=1, plus whether the
/// channel is an official "<artist> - Topic" or VEVO channel.
pub fn artist_presence(artists: &[String], candidate_title: &str, channel: &str) -> (f32, bool) {
    let Some(primary) = artists.first() else {
        return (0.0, false);
    };
    let artist = compact(primary);
    if artist.is_empty() {
        return (0.0, false);
    }
    let title_c = compact(candidate_title);
    let channel_n = normalize(channel);
    let channel_c = channel_n.replace(' ', "");
    let official = (channel_n.ends_with(" topic") && channel_c.starts_with(&artist))
        || (channel_c.contains("vevo") && channel_c.contains(&artist));
    let present = if title_c.contains(&artist) || channel_c.contains(&artist) {
        1.0
    } else {
        // Partial: share of the artist's words found anywhere.
        let words = tokens(primary);
        let hay: HashSet<String> = tokens(candidate_title)
            .into_iter()
            .chain(tokens(channel))
            .collect();
        words.iter().filter(|w| hay.contains(*w)).count() as f32 / words.len().max(1) as f32
    };
    (present, official)
}

/// 1.0 within ±5 s, then falling linearly to 0 at 65 s off. Never negative:
/// far-off durations are penalised, not disqualified.
pub fn duration_score(diff_secs: Option<i64>) -> f32 {
    match diff_secs {
        None => 0.3,
        Some(d) => {
            let d = d.unsigned_abs();
            if d <= DURATION_TOLERANCE_SECS {
                1.0
            } else {
                (1.0 - (d - DURATION_TOLERANCE_SECS) as f32 / 60.0).max(0.0)
            }
        }
    }
}

/// Score one candidate; `None` if it is excluded (live/cover/remix).
pub fn score(track: &TrackQuery<'_>, c: &Candidate) -> Option<Scored> {
    if is_excluded(&c.title, track.title) {
        return None;
    }
    let title = title_similarity(track.title, &c.title, track.artists);
    let (artist, official) = artist_presence(track.artists, &c.title, &c.channel);
    let diff = c
        .duration_secs
        .map(|d| d as i64 - track.duration_secs() as i64);
    let dur = duration_score(diff);
    let boost = if official { 0.08 } else { 0.0 };
    let score = (0.60 * title + 0.20 * artist + 0.20 * dur + boost).clamp(0.0, 1.0);
    Some(Scored {
        candidate: c.clone(),
        score,
        title_similarity: title,
        duration_diff_secs: diff,
    })
}

fn within_tolerance(diff: Option<i64>) -> bool {
    diff.is_some_and(|d| d.unsigned_abs() <= DURATION_TOLERANCE_SECS)
}

/// Pick the best candidate and classify the track.
///
/// - **Matched**: best score ≥ [`HIGH_THRESHOLD`] and within ±5 s.
/// - **Uncertain**: usable, but below the threshold or outside ±5 s.
/// - **Not found**: nothing usable; not added.
pub fn choose(track: &TrackQuery<'_>, candidates: &[Candidate]) -> MatchOutcome {
    let mut best: Option<Scored> = None;
    for c in candidates {
        let Some(s) = score(track, c) else { continue };
        if s.title_similarity < MIN_TITLE {
            continue;
        }
        // Prefer higher score; break ties by closer duration, then order.
        let better = match &best {
            None => true,
            Some(b) => {
                s.score > b.score + 1e-6
                    || ((s.score - b.score).abs() <= 1e-6
                        && s.duration_diff_secs.map(i64::abs).unwrap_or(i64::MAX)
                            < b.duration_diff_secs.map(i64::abs).unwrap_or(i64::MAX))
            }
        };
        if better {
            best = Some(s);
        }
    }
    match best {
        Some(b) if b.score >= MIN_USABLE => {
            let in_tol = within_tolerance(b.duration_diff_secs);
            let status = if b.score >= HIGH_THRESHOLD && in_tol {
                MatchStatus::Matched
            } else {
                MatchStatus::Uncertain
            };
            let note = match status {
                MatchStatus::Matched => None,
                _ if !in_tol && b.duration_diff_secs.is_some() => Some("Duration far off".into()),
                _ if b.duration_diff_secs.is_none() => Some("Duration unknown".into()),
                _ if b.title_similarity < 0.8 => Some("Low title similarity".into()),
                _ => Some("Weak match".into()),
            };
            MatchOutcome {
                status,
                best: Some(b),
                note,
            }
        }
        _ => MatchOutcome {
            status: MatchStatus::NotFound,
            best: None,
            note: Some(if candidates.is_empty() {
                "No search results".into()
            } else {
                "No usable result".into()
            }),
        },
    }
}

/// "+3 s", "-6:12", "0 s" for the report.
pub fn fmt_diff(diff: Option<i64>) -> String {
    match diff {
        None => "–".into(),
        Some(0) => "0 s".into(),
        Some(d) if d.unsigned_abs() < 60 => format!("{d:+} s"),
        Some(d) => {
            let sign = if d < 0 { '-' } else { '+' };
            let a = d.unsigned_abs();
            format!("{sign}{}:{:02}", a / 60, a % 60)
        }
    }
}
