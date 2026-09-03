//! DST-aware timezone resolution for report/dashboard date grouping.
//!
//! Historically [`crate::query::ReportTimezone::Local`] captured a single
//! `Local::now().offset()` snapshot and reused it for every date in the query.
//! In a DST region that is wrong for half the year: winter rows were grouped
//! with a summer offset (or vice versa), so events near midnight landed in the
//! wrong local day, week and month, and per-period cost totals were skewed.
//!
//! This module resolves the machine's IANA zone once per query and evaluates the
//! offset *per timestamp* using the tz database. `Utc` and `Fixed` keep using a
//! plain SQLite time modifier so their generated SQL — and therefore their
//! behavior — is byte-for-byte unchanged.

use chrono::{DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

pub(crate) use crate::store::sqlite_functions::{
    FN_LOCAL_DATE, FN_LOCAL_HOUR, FN_LOCAL_MONTH, FN_LOCAL_WEEK,
};

/// A timezone resolved to something that can answer "what was the offset on
/// this specific instant".
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedZone {
    /// Fixed offset (covers `Utc` and `Fixed`). Offset is constant for all dates.
    Fixed(FixedOffset),
    /// Full IANA zone with historical DST rules.
    Iana(Tz),
}

impl ResolvedZone {
    /// Resolves the machine's local timezone to an IANA zone when possible.
    ///
    /// Falls back to the current fixed-offset snapshot when the platform does
    /// not report a zone name or the name is absent from the tz database. That
    /// fallback reproduces the old (DST-naive) behavior rather than failing the
    /// query.
    pub(crate) fn local() -> Self {
        match iana_time_zone::get_timezone() {
            Ok(name) => match name.parse::<Tz>() {
                Ok(tz) => Self::Iana(tz),
                Err(_) => Self::Fixed(local_offset_snapshot()),
            },
            Err(_) => Self::Fixed(local_offset_snapshot()),
        }
    }

    /// The local calendar date at a UTC instant.
    pub(crate) fn date_at(&self, instant: DateTime<Utc>) -> NaiveDate {
        match self {
            Self::Fixed(offset) => instant.with_timezone(offset).date_naive(),
            Self::Iana(tz) => instant.with_timezone(tz).date_naive(),
        }
    }

    /// The UTC instant at which the given local calendar date begins.
    ///
    /// Both DST edge cases are handled explicitly:
    /// - **fall-back** (the local time occurs twice): the *earliest* instant is
    ///   used, so an inclusive lower bound covers the whole repeated hour.
    /// - **spring-forward** (the local time does not exist at all, which happens
    ///   in zones that shift exactly at midnight): the first instant that does
    ///   exist after the gap is used, i.e. the transition itself.
    pub(crate) fn local_date_start_utc(&self, date: NaiveDate) -> DateTime<Utc> {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always a valid NaiveDateTime");
        match self {
            Self::Fixed(offset) => resolve_local(offset, midnight),
            Self::Iana(tz) => resolve_local(tz, midnight),
        }
    }

    /// SQL expression yielding the local `YYYY-MM-DD` for `column`.
    ///
    /// Fixed offsets keep the original `date(col, '+N seconds')` form so their
    /// query plans and results are unchanged. IANA zones use the registered
    /// scalar function, which consults the tz database per row.
    pub(crate) fn local_date_expr(&self, column: &str) -> String {
        match self {
            Self::Fixed(offset) => {
                format!("date({column}, '{}')", seconds_modifier(*offset))
            }
            Self::Iana(tz) => format!("{FN_LOCAL_DATE}({column}, '{}')", tz.name()),
        }
    }

    /// SQL expression yielding the local `YYYY-MM` for `column`.
    pub(crate) fn local_month_expr(&self, column: &str) -> String {
        match self {
            Self::Fixed(offset) => {
                format!(
                    "strftime('%Y-%m', {column}, '{}')",
                    seconds_modifier(*offset)
                )
            }
            Self::Iana(tz) => format!("{FN_LOCAL_MONTH}({column}, '{}')", tz.name()),
        }
    }

    /// SQL expression yielding the local `YYYY-WW` week key for `column`.
    pub(crate) fn local_week_expr(&self, column: &str) -> String {
        match self {
            Self::Fixed(offset) => {
                format!(
                    "strftime('%Y-%W', {column}, '{}')",
                    seconds_modifier(*offset)
                )
            }
            Self::Iana(tz) => format!("{FN_LOCAL_WEEK}({column}, '{}')", tz.name()),
        }
    }

    /// SQL expression yielding the local clock hour `YYYY-MM-DD HH:00` for `column`.
    pub(crate) fn local_hour_expr(&self, column: &str) -> String {
        match self {
            Self::Fixed(offset) => {
                format!(
                    "strftime('%Y-%m-%d %H:00', {column}, '{}')",
                    seconds_modifier(*offset)
                )
            }
            Self::Iana(tz) => format!("{FN_LOCAL_HOUR}({column}, '{}')", tz.name()),
        }
    }
}

fn local_offset_snapshot() -> FixedOffset {
    use chrono::Offset;
    chrono::Local::now().offset().fix()
}

fn seconds_modifier(offset: FixedOffset) -> String {
    let seconds = offset.local_minus_utc();
    if seconds >= 0 {
        format!("+{seconds} seconds")
    } else {
        format!("{seconds} seconds")
    }
}

/// Resolves a naive local datetime to a UTC instant, handling both DST edges.
fn resolve_local<Tzz: TimeZone>(tz: &Tzz, local: NaiveDateTime) -> DateTime<Utc> {
    use chrono::offset::LocalResult;
    match tz.from_local_datetime(&local) {
        LocalResult::Single(value) => value.with_timezone(&Utc),
        // Repeated local time: take the first occurrence.
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        // Non-existent local time (gap). Probe forward to the first instant
        // that exists; gaps are at most a few hours, so this terminates fast.
        LocalResult::None => {
            let mut probe = local;
            for _ in 0..(4 * 60 / 15) {
                probe += Duration::minutes(15);
                match tz.from_local_datetime(&probe) {
                    LocalResult::Single(value) => return value.with_timezone(&Utc),
                    LocalResult::Ambiguous(earliest, _) => return earliest.with_timezone(&Utc),
                    LocalResult::None => continue,
                }
            }
            // Should be unreachable for real tz data; fall back to treating the
            // wall-clock time as UTC rather than panicking in a query path.
            Utc.from_utc_datetime(&local)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    use crate::store::sqlite_functions::{format_local_week, parse_fast_utc, register_functions};

    const NY: Tz = chrono_tz::America::New_York;

    fn utc(raw: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(raw)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    fn conn_with_functions() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        register_functions(&conn).expect("register");
        conn
    }

    fn local_date_via_sql(conn: &Connection, stored: &str, zone: &str) -> Option<String> {
        conn.query_row(
            &format!("SELECT {FN_LOCAL_DATE}(?1, ?2)"),
            (stored, zone),
            |row| row.get::<_, Option<String>>(0),
        )
        .expect("query ok")
    }

    /// DATA-003 core regression: with one fixed offset, only one of these two
    /// dates can be right. New York is UTC-5 in January and UTC-4 in July, so
    /// 03:30Z belongs to the *previous* local day in winter but the *same* local
    /// day in summer.
    #[test]
    fn historical_dates_use_the_offset_that_was_actually_in_effect() {
        let zone = ResolvedZone::Iana(NY);
        assert_eq!(
            zone.date_at(utc("2026-01-15T03:30:00Z")).to_string(),
            "2026-01-14",
            "winter: UTC-5 puts 03:30Z on the previous local day"
        );
        assert_eq!(
            zone.date_at(utc("2026-07-15T03:30:00Z")).to_string(),
            "2026-07-14",
            "summer: UTC-4 still puts 03:30Z on the previous local day"
        );
        // The discriminating pair: 04:30Z.
        assert_eq!(
            zone.date_at(utc("2026-01-15T04:30:00Z")).to_string(),
            "2026-01-14",
            "winter UTC-5: 04:30Z is 23:30 previous day"
        );
        assert_eq!(
            zone.date_at(utc("2026-07-15T04:30:00Z")).to_string(),
            "2026-07-15",
            "summer UTC-4: 04:30Z is 00:30 same day"
        );
    }

    /// Proves the pre-DATA-003 approach cannot satisfy the assertions above.
    ///
    /// The old code took one `Local::now().offset()` snapshot and reused it for
    /// every date. This reproduces that with both possible snapshots and shows
    /// each one gets the other half of the year wrong — so the fix is not merely
    /// a refactor, it changes results that were previously incorrect.
    #[test]
    fn single_offset_snapshot_is_wrong_for_half_the_year() {
        let winter_snapshot = ResolvedZone::Fixed(FixedOffset::west_opt(5 * 3600).unwrap());
        let summer_snapshot = ResolvedZone::Fixed(FixedOffset::west_opt(4 * 3600).unwrap());
        let winter_instant = utc("2026-01-15T04:30:00Z");
        let summer_instant = utc("2026-07-15T04:30:00Z");

        // A winter (UTC-5) snapshot gets January right but July wrong.
        assert_eq!(
            winter_snapshot.date_at(winter_instant).to_string(),
            "2026-01-14"
        );
        assert_ne!(
            winter_snapshot.date_at(summer_instant).to_string(),
            "2026-07-15",
            "a UTC-5 snapshot misfiles the July instant"
        );

        // A summer (UTC-4) snapshot gets July right but January wrong.
        assert_eq!(
            summer_snapshot.date_at(summer_instant).to_string(),
            "2026-07-15"
        );
        assert_ne!(
            summer_snapshot.date_at(winter_instant).to_string(),
            "2026-01-14",
            "a UTC-4 snapshot misfiles the January instant"
        );

        // The IANA zone gets both right, which no fixed offset can do.
        let zone = ResolvedZone::Iana(NY);
        assert_eq!(zone.date_at(winter_instant).to_string(), "2026-01-14");
        assert_eq!(zone.date_at(summer_instant).to_string(), "2026-07-15");
    }

    /// Same discriminating pair through the SQL function, which is what the
    /// queries actually execute.
    #[test]
    fn sql_function_groups_dst_boundaries_by_real_offset() {
        let conn = conn_with_functions();
        assert_eq!(
            local_date_via_sql(&conn, "2026-01-15T04:30:00Z", "America/New_York").as_deref(),
            Some("2026-01-14")
        );
        assert_eq!(
            local_date_via_sql(&conn, "2026-07-15T04:30:00Z", "America/New_York").as_deref(),
            Some("2026-07-15")
        );
    }

    /// Guards against the function silently returning NULL, which would make
    /// grouping collapse and look fast while being wrong.
    #[test]
    fn sql_function_returns_values_for_all_stored_timestamp_shapes() {
        let conn = conn_with_functions();
        for stored in [
            "2026-07-15T04:30:00Z",
            "2026-07-15 04:30:00",
            "2026-07-15T04:30:00",
            "2026-07-15T04:30:00.123Z",
            "2026-07-15T04:30:00+00:00",
        ] {
            assert!(
                local_date_via_sql(&conn, stored, "America/New_York").is_some(),
                "stored form {stored} must not yield NULL"
            );
        }
    }

    #[test]
    fn unknown_zone_yields_sql_error_rather_than_silent_null() {
        let conn = conn_with_functions();
        let result = conn.query_row(
            &format!("SELECT {FN_LOCAL_DATE}(?1, ?2)"),
            ("2026-07-15T04:30:00Z", "Not/AZone"),
            |row| row.get::<_, Option<String>>(0),
        );
        assert!(result.is_err(), "unknown zone must surface as an error");
    }

    /// Fall-back: 01:30 local occurs twice on 2026-11-01 in New York. Both
    /// instants must land on the same local date.
    #[test]
    fn fall_back_repeated_hour_stays_on_one_local_date() {
        let zone = ResolvedZone::Iana(NY);
        // 05:30Z = 01:30 EDT (first pass), 06:30Z = 01:30 EST (second pass).
        assert_eq!(
            zone.date_at(utc("2026-11-01T05:30:00Z")).to_string(),
            "2026-11-01"
        );
        assert_eq!(
            zone.date_at(utc("2026-11-01T06:30:00Z")).to_string(),
            "2026-11-01"
        );
    }

    /// Spring-forward: 02:00-03:00 local does not exist on 2026-03-08.
    /// Timestamps on either side must still map to that local date.
    #[test]
    fn spring_forward_gap_maps_surrounding_instants_to_the_same_date() {
        let zone = ResolvedZone::Iana(NY);
        assert_eq!(
            zone.date_at(utc("2026-03-08T06:30:00Z")).to_string(),
            "2026-03-08",
            "01:30 EST, before the gap"
        );
        assert_eq!(
            zone.date_at(utc("2026-03-08T07:30:00Z")).to_string(),
            "2026-03-08",
            "03:30 EDT, after the gap"
        );
    }

    /// Day-start bounds must use the real offset for that date, so a winter
    /// bound and a summer bound differ by an hour in UTC.
    #[test]
    fn local_day_start_tracks_dst_for_range_bounds() {
        let zone = ResolvedZone::Iana(NY);
        let winter = zone.local_date_start_utc(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap());
        let summer = zone.local_date_start_utc(NaiveDate::from_ymd_opt(2026, 7, 15).unwrap());
        assert_eq!(winter.to_rfc3339(), "2026-01-15T05:00:00+00:00", "UTC-5");
        assert_eq!(summer.to_rfc3339(), "2026-07-15T04:00:00+00:00", "UTC-4");
    }

    /// A zone whose DST transition happens exactly at midnight, so local
    /// midnight itself does not exist. The bound must be the transition
    /// instant, not a panic and not a silently wrong value.
    #[test]
    fn local_day_start_handles_midnight_transition_gap() {
        let beirut = ResolvedZone::Iana(chrono_tz::Asia::Beirut);
        let date = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
        let start = beirut.local_date_start_utc(date);
        // Whatever the exact instant, it must be the first one whose local date
        // is the requested date, and it must not precede the requested day.
        assert_eq!(beirut.date_at(start), date);
        assert!(start >= utc("2026-03-28T22:00:00Z"));
    }

    /// Fixed and UTC zones must keep emitting the exact SQL they emitted before
    /// DATA-003, so their behavior is provably unchanged.
    #[test]
    fn fixed_and_utc_sql_is_unchanged_native_sqlite() {
        let utc_zone = ResolvedZone::Fixed(FixedOffset::east_opt(0).unwrap());
        assert_eq!(
            utc_zone.local_date_expr("hour_start"),
            "date(hour_start, '+0 seconds')"
        );
        assert_eq!(
            utc_zone.local_month_expr("hour_start"),
            "strftime('%Y-%m', hour_start, '+0 seconds')"
        );
        assert_eq!(
            utc_zone.local_week_expr("hour_start"),
            "strftime('%Y-%W', hour_start, '+0 seconds')"
        );
        assert_eq!(
            utc_zone.local_hour_expr("hour_start"),
            "strftime('%Y-%m-%d %H:00', hour_start, '+0 seconds')"
        );

        let plus8 = ResolvedZone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap());
        assert_eq!(
            plus8.local_date_expr("event_at"),
            "date(event_at, '+28800 seconds')"
        );
        let minus5 = ResolvedZone::Fixed(FixedOffset::east_opt(-5 * 3600).unwrap());
        assert_eq!(
            minus5.local_date_expr("event_at"),
            "date(event_at, '-18000 seconds')"
        );
    }

    /// IANA zones must route to the scalar function instead.
    #[test]
    fn iana_sql_uses_the_dst_aware_function() {
        let zone = ResolvedZone::Iana(NY);
        assert_eq!(
            zone.local_date_expr("hour_start"),
            format!("{FN_LOCAL_DATE}(hour_start, 'America/New_York')")
        );
        assert_eq!(
            zone.local_hour_expr("hour_start"),
            format!("{FN_LOCAL_HOUR}(hour_start, 'America/New_York')")
        );
    }

    #[test]
    fn local_hour_sql_matches_fixed_strftime_and_merges_half_hours() {
        let conn = conn_with_functions();
        let plus8 = ResolvedZone::Fixed(FixedOffset::east_opt(8 * 3600).unwrap());
        let expr = plus8.local_hour_expr("?1");
        let hour: String = conn
            .query_row(&format!("SELECT {expr}"), ["2026-04-04T16:30:00Z"], |row| {
                row.get(0)
            })
            .expect("query ok");
        assert_eq!(hour, "2026-04-05 00:00");

        let ny_six: Option<String> = conn
            .query_row(
                &format!("SELECT {FN_LOCAL_HOUR}(?1, ?2)"),
                ("2026-03-08T06:00:00Z", "America/New_York"),
                |row| row.get(0),
            )
            .expect("query ok");
        let ny_six_thirty: Option<String> = conn
            .query_row(
                &format!("SELECT {FN_LOCAL_HOUR}(?1, ?2)"),
                ("2026-03-08T06:30:00Z", "America/New_York"),
                |row| row.get(0),
            )
            .expect("query ok");
        assert_eq!(ny_six.as_deref(), Some("2026-03-08 01:00"));
        assert_eq!(ny_six_thirty, ny_six);

        let after_spring: Option<String> = conn
            .query_row(
                &format!("SELECT {FN_LOCAL_HOUR}(?1, ?2)"),
                ("2026-03-08T07:00:00Z", "America/New_York"),
                |row| row.get(0),
            )
            .expect("query ok");
        assert_eq!(after_spring.as_deref(), Some("2026-03-08 03:00"));
    }

    /// The `%W` week number is computed in Rust for IANA zones but by SQLite
    /// for fixed offsets. They must agree, or week grouping would shift
    /// depending on which timezone kind was selected.
    #[test]
    fn rust_week_number_matches_sqlite_strftime() {
        let conn = conn_with_functions();
        // Compare against SQLite for a UTC zone across a full year, including
        // year boundaries and both DST edges.
        for day in 0..366 {
            let date = NaiveDate::from_ymd_opt(2026, 1, 1)
                .unwrap()
                .checked_add_signed(Duration::days(day))
                .unwrap();
            let stored = format!("{}T12:00:00Z", date.format("%Y-%m-%d"));
            let sqlite: String = conn
                .query_row(
                    "SELECT strftime('%Y-%W', ?1, '+0 seconds')",
                    [&stored],
                    |row| row.get(0),
                )
                .expect("sqlite week");
            let ours = format_local_week(date);
            assert_eq!(ours, sqlite, "week mismatch for {stored}");
        }
    }

    /// A zone with a non-hour offset must still produce correct dates.
    #[test]
    fn half_hour_offset_zones_are_handled() {
        let kolkata = ResolvedZone::Iana(chrono_tz::Asia::Kolkata);
        // UTC+5:30 year-round: 18:45Z is 00:15 the next local day.
        assert_eq!(
            kolkata.date_at(utc("2026-01-15T18:45:00Z")).to_string(),
            "2026-01-16"
        );
        assert_eq!(
            kolkata.date_at(utc("2026-01-15T18:15:00Z")).to_string(),
            "2026-01-15"
        );
    }

    /// A no-DST zone must give the same answers all year (acceptance criterion:
    /// fixed-offset regions unchanged).
    #[test]
    fn no_dst_zone_is_stable_across_the_year() {
        let shanghai = ResolvedZone::Iana(chrono_tz::Asia::Shanghai);
        for (instant, expected) in [
            ("2026-01-15T16:30:00Z", "2026-01-16"),
            ("2026-07-15T16:30:00Z", "2026-07-16"),
            ("2026-01-15T15:30:00Z", "2026-01-15"),
            ("2026-07-15T15:30:00Z", "2026-07-15"),
        ] {
            assert_eq!(
                shanghai.date_at(utc(instant)).to_string(),
                expected,
                "UTC+8 is stable, {instant}"
            );
        }
    }

    #[test]
    fn unparseable_timestamps_yield_null_not_an_error() {
        let conn = conn_with_functions();
        assert_eq!(
            local_date_via_sql(&conn, "not-a-timestamp", "America/New_York"),
            None
        );
    }

    #[test]
    fn fast_path_parser_agrees_with_chrono_on_stored_shapes() {
        for stored in [
            "2026-07-15T04:30:00Z",
            "2026-01-01T00:00:00Z",
            "2026-12-31T23:59:59Z",
        ] {
            let fast = parse_fast_utc(stored.as_bytes()).expect("fast path");
            let slow = DateTime::parse_from_rfc3339(stored)
                .expect("chrono")
                .with_timezone(&Utc);
            assert_eq!(fast, slow, "mismatch for {stored}");
        }
    }
}
