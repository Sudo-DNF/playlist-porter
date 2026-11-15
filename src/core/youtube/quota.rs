//! YouTube Data API quota: costs, an in-process estimate of units used
//! today, and the daily reset at midnight Pacific time.
//!
//! This is YouTube API quota bookkeeping, not analysis of Spotify data.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};

pub const DAILY_LIMIT: u32 = 10_000;
pub const COST_SEARCH: u32 = 100;
pub const COST_VIDEOS: u32 = 1;
pub const COST_CHANNELS: u32 = 1;
pub const COST_PLAYLIST_INSERT: u32 = 50;
pub const COST_ITEM_INSERT: u32 = 50;
/// search + videos + playlistItems.insert, rounded as documented (~150).
pub const COST_PER_TRACK: u32 = COST_SEARCH + COST_VIDEOS + COST_ITEM_INSERT;

/// Units needed for `tracks` more tracks, plus the playlist if it doesn't exist yet.
pub fn estimate_units(tracks: usize, create_playlist: bool) -> u32 {
    let per = 150u32; // documented estimate: ~150 per track
    per.saturating_mul(tracks as u32)
        + if create_playlist {
            COST_PLAYLIST_INSERT
        } else {
            0
        }
}

/// Calendar days needed, given what's already been used today.
pub fn days_needed(units: u32, used_today: u32) -> u32 {
    let left_today = DAILY_LIMIT.saturating_sub(used_today);
    if units <= left_today {
        1
    } else {
        1 + (units - left_today).div_ceil(DAILY_LIMIT)
    }
}

// ---------------------------------------------------------------------------
// Pacific time without a timezone database. US rules since 2007: DST starts
// the second Sunday of March and ends the first Sunday of November.

fn nth_weekday(year: i32, month: u32, weekday: Weekday, n: u32) -> NaiveDate {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let offset = (7 + weekday.num_days_from_monday() - first.weekday().num_days_from_monday()) % 7;
    first + Duration::days((offset + 7 * (n - 1)) as i64)
}

/// UTC offset (hours) of US Pacific time at this instant.
pub fn pacific_offset_hours(at: DateTime<Utc>) -> i64 {
    let y = at.year();
    // 2:00 PST = 10:00 UTC on the DST start day; 2:00 PDT = 09:00 UTC on the end day.
    let start = nth_weekday(y, 3, Weekday::Sun, 2)
        .and_hms_opt(10, 0, 0)
        .expect("valid time")
        .and_utc();
    let end = nth_weekday(y, 11, Weekday::Sun, 1)
        .and_hms_opt(9, 0, 0)
        .expect("valid time")
        .and_utc();
    if at >= start && at < end {
        -7
    } else {
        -8
    }
}

/// The current date in US Pacific time (the quota "day").
pub fn pacific_date(at: DateTime<Utc>) -> NaiveDate {
    (at + Duration::hours(pacific_offset_hours(at))).date_naive()
}

/// The next midnight in US Pacific time, as a UTC instant.
pub fn next_reset(at: DateTime<Utc>) -> DateTime<Utc> {
    let tomorrow = pacific_date(at) + Duration::days(1);
    let midnight = tomorrow.and_hms_opt(0, 0, 0).expect("valid time");
    // Midnight is never inside a DST switch (they happen at 2:00), so the
    // offset just after midnight is the offset for that whole hour.
    let guess = Utc.from_utc_datetime(&(midnight + Duration::hours(8)));
    let off = pacific_offset_hours(guess);
    Utc.from_utc_datetime(&(midnight - Duration::hours(off)))
}

/// "3:00 AM" in the user's local time zone.
pub fn reset_local_label(at: DateTime<Utc>) -> String {
    let local = next_reset(at).with_timezone(&chrono::Local);
    local.format("%-I:%M %p").to_string()
}

/// Estimated units used today, in this process. Resets with the Pacific date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaTracker {
    used: u32,
    day: NaiveDate,
}

impl QuotaTracker {
    pub fn new(now: DateTime<Utc>) -> Self {
        QuotaTracker {
            used: 0,
            day: pacific_date(now),
        }
    }

    fn roll(&mut self, now: DateTime<Utc>) {
        let today = pacific_date(now);
        if today != self.day {
            self.day = today;
            self.used = 0;
        }
    }

    pub fn used(&mut self, now: DateTime<Utc>) -> u32 {
        self.roll(now);
        self.used
    }

    pub fn remaining(&mut self, now: DateTime<Utc>) -> u32 {
        DAILY_LIMIT.saturating_sub(self.used(now))
    }

    /// Whether `cost` more units fit in today's budget.
    pub fn can_afford(&mut self, cost: u32, now: DateTime<Utc>) -> bool {
        self.remaining(now) >= cost
    }

    pub fn spend(&mut self, cost: u32, now: DateTime<Utc>) {
        self.roll(now);
        self.used = self.used.saturating_add(cost).min(DAILY_LIMIT);
    }

    /// YouTube said `quotaExceeded`: treat today's budget as gone.
    pub fn exhaust(&mut self, now: DateTime<Utc>) {
        self.roll(now);
        self.used = DAILY_LIMIT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn estimates_match_docs() {
        assert_eq!(estimate_units(87, true), 13_100);
        assert_eq!(estimate_units(1, false), 150);
        // About 65 tracks a day.
        assert_eq!(DAILY_LIMIT / 150, 66);
        assert_eq!(days_needed(13_100, 0), 2);
        assert_eq!(days_needed(9_000, 0), 1);
        assert_eq!(days_needed(9_000, 2_000), 2);
        assert_eq!(days_needed(25_000, 0), 3);
    }

    #[test]
    fn pacific_offsets_follow_dst() {
        assert_eq!(pacific_offset_hours(utc("2026-01-15T12:00:00Z")), -8);
        assert_eq!(pacific_offset_hours(utc("2026-07-15T12:00:00Z")), -7);
        // 2026: DST from Sun 8 Mar 10:00 UTC to Sun 1 Nov 09:00 UTC.
        assert_eq!(pacific_offset_hours(utc("2026-03-08T09:59:59Z")), -8);
        assert_eq!(pacific_offset_hours(utc("2026-03-08T10:00:00Z")), -7);
        assert_eq!(pacific_offset_hours(utc("2026-11-01T08:59:59Z")), -7);
        assert_eq!(pacific_offset_hours(utc("2026-11-01T09:00:00Z")), -8);
    }

    #[test]
    fn reset_is_next_pacific_midnight() {
        // Winter: midnight PST = 08:00 UTC.
        assert_eq!(
            next_reset(utc("2026-11-14T20:00:00Z")),
            utc("2026-11-15T08:00:00Z")
        );
        // Just before midnight Pacific (07:59 UTC) the reset is a minute away.
        assert_eq!(
            next_reset(utc("2026-11-15T07:59:00Z")),
            utc("2026-11-15T08:00:00Z")
        );
        // Summer: midnight PDT = 07:00 UTC.
        assert_eq!(
            next_reset(utc("2026-07-01T12:00:00Z")),
            utc("2026-07-02T07:00:00Z")
        );
        // Night of the DST start: midnight 8 Mar is still PST.
        assert_eq!(
            next_reset(utc("2026-03-07T20:00:00Z")),
            utc("2026-03-08T08:00:00Z")
        );
        // Night of the DST end: midnight 1 Nov is still PDT.
        assert_eq!(
            next_reset(utc("2026-10-31T20:00:00Z")),
            utc("2026-11-01T07:00:00Z")
        );
    }

    #[test]
    fn tracker_rolls_over_at_pacific_midnight() {
        let t0 = utc("2026-11-15T07:00:00Z"); // 23:00 PST on the 14th
        let mut q = QuotaTracker::new(t0);
        q.spend(9_900, t0);
        assert!(!q.can_afford(151, t0));
        assert!(q.can_afford(100, t0));
        let t1 = utc("2026-11-15T08:00:01Z"); // just after midnight PST
        assert_eq!(q.used(t1), 0);
        q.exhaust(t1);
        assert_eq!(q.remaining(t1), 0);
    }
}
