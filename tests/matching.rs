//! Matching rules: exclusion, duration preference, normalisation,
//! thresholds and ISO-8601 parsing. Pure functions, no network.

use playlist_porter::core::matching::*;

fn cand(id: &str, title: &str, channel: &str, secs: Option<u64>) -> Candidate {
    Candidate {
        video_id: id.into(),
        title: title.into(),
        channel: channel.into(),
        duration_secs: secs,
    }
}

fn artists(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

// --- ISO-8601 -----------------------------------------------------------

#[test]
fn iso8601_durations() {
    assert_eq!(parse_iso8601_duration("PT4M13S"), Some(253));
    assert_eq!(parse_iso8601_duration("PT45S"), Some(45));
    assert_eq!(parse_iso8601_duration("PT1H2M"), Some(3720));
    assert_eq!(parse_iso8601_duration("PT1H0M5S"), Some(3605));
    assert_eq!(parse_iso8601_duration("P1DT2H"), Some(93_600));
    assert_eq!(parse_iso8601_duration("P0D"), Some(0));
    assert_eq!(parse_iso8601_duration("PT0S"), Some(0));
    assert_eq!(parse_iso8601_duration("PT3.6S"), Some(4));
    for bad in ["", "P", "PT", "4M13S", "PT4X", "PT4M13", "P1Y"] {
        assert_eq!(parse_iso8601_duration(bad), None, "{bad}");
    }
}

// --- Query normalisation --------------------------------------------------

#[test]
fn strips_remaster_and_feat_from_query() {
    assert_eq!(strip_suffixes("Dreams - 2004 Remaster"), "Dreams");
    assert_eq!(
        strip_suffixes("An Ending (Ascent) - Remastered 2005"),
        "An Ending (Ascent)"
    );
    assert_eq!(strip_suffixes("Ride (feat. Kim Example)"), "Ride");
    assert_eq!(strip_suffixes("Ride [feat. Kim Example]"), "Ride");
    assert_eq!(strip_suffixes("Ride feat. Kim Example"), "Ride");
    assert_eq!(strip_suffixes("Song (with Someone)"), "Song");
    assert_eq!(strip_suffixes("Song - Radio Edit"), "Song");
    assert_eq!(strip_suffixes("Song - Mono"), "Song");
    assert_eq!(strip_suffixes("Song - 2011"), "Song");
    // Version words that matter stay.
    assert_eq!(
        strip_suffixes("Song - Live at Wembley"),
        "Song - Live at Wembley"
    );
    assert_eq!(strip_suffixes("Song (Remix)"), "Song (Remix)");
    assert_eq!(strip_suffixes("Strobe"), "Strobe");
}

#[test]
fn query_is_primary_artist_plus_stripped_title() {
    assert_eq!(
        build_query("Fleetwood Mac", "Dreams - 2004 Remaster"),
        "Fleetwood Mac Dreams"
    );
    assert_eq!(
        build_query("The Roadside Saints", "Ride (feat. Kim Example)"),
        "The Roadside Saints Ride"
    );
}

#[test]
fn normalisation() {
    assert_eq!(
        normalize("Sigur Rós – Svefn-g-englar!"),
        "sigur ros svefn g englar"
    );
    assert_eq!(normalize("Simon & Garfunkel"), "simon and garfunkel");
    assert_eq!(normalize("Don't Stop"), "dont stop");
}

// --- Exclusion ----------------------------------------------------------

#[test]
fn live_cover_remix_are_excluded_whole_word_only() {
    assert!(is_excluded("Song (Live at Wembley)", "Song"));
    assert!(is_excluded("Song - acoustic COVER", "Song"));
    assert!(is_excluded("Song [Remix]", "Song"));
    // Unless the Spotify title has the same word.
    assert!(!is_excluded("Song (Live)", "Song - Live"));
    assert!(!is_excluded("Song (Remix)", "Song - Remix"));
    // Whole words only.
    assert!(!is_excluded("Alive", "Alive"));
    assert!(!is_excluded("Discovery", "Discovery"));
    assert!(!is_excluded("Remixed Feelings", "Song"));
    assert!(!is_excluded("Delivered", "Delivered"));
}

#[test]
fn excluded_candidates_are_never_chosen() {
    let a = artists(&["Example Band"]);
    let t = TrackQuery {
        title: "Harbour Lights",
        artists: &a,
        duration_ms: 240_000,
    };
    let out = choose(
        &t,
        &[cand(
            "live",
            "Example Band - Harbour Lights (Live)",
            "Example Band",
            Some(240),
        )],
    );
    assert_eq!(out.status, MatchStatus::NotFound);
}

// --- Duration preference --------------------------------------------------

#[test]
fn within_five_seconds_is_preferred() {
    let a = artists(&["Example Band"]);
    let t = TrackQuery {
        title: "Harbour Lights",
        artists: &a,
        duration_ms: 240_000,
    };
    let far = cand(
        "far",
        "Example Band - Harbour Lights",
        "Example Band",
        Some(300),
    );
    let near = cand(
        "near",
        "Example Band - Harbour Lights",
        "Example Band",
        Some(244),
    );
    let out = choose(&t, &[far, near]);
    assert_eq!(out.best.unwrap().candidate.video_id, "near");
    assert_eq!(out.status, MatchStatus::Matched);
}

#[test]
fn far_duration_is_penalised_not_disqualified() {
    let a = artists(&["Ambient Person"]);
    let t = TrackQuery {
        title: "An Ending (Ascent) - Remastered 2005",
        artists: &a,
        duration_ms: 265_000,
    };
    let out = choose(
        &t,
        &[cand(
            "hour",
            "Ambient Person - An Ending (Ascent) [1 Hour]",
            "Ambient Archive",
            Some(3600),
        )],
    );
    assert_eq!(out.status, MatchStatus::Uncertain);
    assert_eq!(out.note.as_deref(), Some("Duration far off"));
    assert!(duration_score(Some(6)) < 1.0);
    assert_eq!(duration_score(Some(5)), 1.0);
    assert_eq!(duration_score(Some(-5)), 1.0);
    assert_eq!(duration_score(Some(1000)), 0.0);
}

// --- Status thresholds -------------------------------------------------------

#[test]
fn topic_channel_exact_match_is_matched() {
    let a = artists(&["Neon Atlas"]);
    let t = TrackQuery {
        title: "City at Midnight",
        artists: &a,
        duration_ms: 243_000,
    };
    let out = choose(
        &t,
        &[cand(
            "t",
            "City at Midnight",
            "Neon Atlas - Topic",
            Some(243),
        )],
    );
    assert_eq!(out.status, MatchStatus::Matched);
    let b = out.best.unwrap();
    assert!(b.score >= HIGH_THRESHOLD);
    assert_eq!(b.duration_diff_secs, Some(0));
}

#[test]
fn vevo_channel_is_recognised() {
    let a = artists(&["Coral Static"]);
    let (present, official) =
        artist_presence(&a, "Electric Tide (Official Video)", "CoralStaticVEVO");
    assert_eq!(present, 1.0);
    assert!(official);
}

#[test]
fn low_title_similarity_is_uncertain() {
    let a = artists(&["Fen & Field"]);
    let t = TrackQuery {
        title: "Svefn-g-englar Morning Song",
        artists: &a,
        duration_ms: 600_000,
    };
    let out = choose(
        &t,
        &[cand(
            "x",
            "fen and field svefn g englar hq",
            "uploader77",
            Some(603),
        )],
    );
    assert_eq!(out.status, MatchStatus::Uncertain);
    assert_eq!(out.note.as_deref(), Some("Low title similarity"));
}

#[test]
fn unrelated_results_are_not_found() {
    let a = artists(&["Nightdesk"]);
    let t = TrackQuery {
        title: "Midnight Coding Session (feat. Loftwave)",
        artists: &a,
        duration_ms: 180_000,
    };
    let out = choose(
        &t,
        &[
            cand("a", "Top 10 cooking tips", "Kitchen Channel", Some(180)),
            cand("b", "Morning yoga flow", "Yoga With Someone", Some(181)),
        ],
    );
    assert_eq!(out.status, MatchStatus::NotFound);
    assert!(out.best.is_none());
    assert_eq!(choose(&t, &[]).note.as_deref(), Some("No search results"));
}

#[test]
fn feat_and_remaster_in_spotify_title_still_match_plain_video_title() {
    let a = artists(&["Fleetwood Mac"]);
    let t = TrackQuery {
        title: "Dreams - 2004 Remaster",
        artists: &a,
        duration_ms: 257_000,
    };
    let out = choose(
        &t,
        &[cand(
            "d",
            "Fleetwood Mac - Dreams (Official Music Video)",
            "Fleetwood Mac",
            Some(258),
        )],
    );
    assert_eq!(out.status, MatchStatus::Matched);
}

#[test]
fn diff_formatting() {
    assert_eq!(fmt_diff(Some(0)), "0 s");
    assert_eq!(fmt_diff(Some(3)), "+3 s");
    assert_eq!(fmt_diff(Some(-2)), "-2 s");
    assert_eq!(fmt_diff(Some(-372)), "-6:12");
    assert_eq!(fmt_diff(Some(3334)), "+55:34");
    assert_eq!(fmt_diff(None), "–");
}
