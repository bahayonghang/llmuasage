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

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, functions::FunctionFlags};

use crate::error::Result;

/// SQL function name for DST-aware local date extraction.
pub(crate) const FN_LOCAL_DATE: &str = "llmusage_local_date";
/// SQL function name for DST-aware local `YYYY-MM` extraction.
pub(crate) const FN_LOCAL_MONTH: &str = "llmusage_local_month";
/// SQL function name for DST-aware local `YYYY-WW` extraction.
pub(crate) const FN_LOCAL_WEEK: &str = "llmusage_local_week";

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

/// Registers the DST-aware date/month functions on a connection.
///
/// Must be called on every connection that may run a `Local`-timezone query.
/// The functions are deterministic for a given (timestamp, zone) pair, so
/// SQLite is free to cache and index-optimize them.
pub(crate) fn register_functions(conn: &Connection) -> Result<()> {
    register_one(conn, FN_LOCAL_DATE, format_local_date)?;
    register_one(conn, FN_LOCAL_MONTH, format_local_month)?;
    register_one(conn, FN_LOCAL_WEEK, format_local_week)?;
    Ok(())
}

// These run once per scanned row, so they format from integer fields directly.
// `chrono`'s `format()` re-parses its format string on every call, which is a
// measurable cost on a full-table scan.

fn format_local_date(date: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year(), date.month(), date.day())
}

fn format_local_month(date: NaiveDate) -> String {
    format!("{:04}-{:02}", date.year(), date.month())
}

/// `%Y-%W` matching C / SQLite `strftime` semantics.
///
/// `%W` is the week number with the first Monday as day 1 of week 1 (00-53).
/// This must agree exactly with the SQLite `strftime('%Y-%W', ...)` used on the
/// fixed-offset path, otherwise the two timezone modes would bucket weeks
/// differently.
fn format_local_week(date: NaiveDate) -> String {
    let ordinal0 = date.ordinal0();
    let from_sunday = date.weekday().num_days_from_sunday();
    let monday_based = if from_sunday == 0 { 6 } else { from_sunday - 1 };
    let week = (ordinal0 + 7 - monday_based) / 7;
    format!("{:04}-{:02}", date.year(), week)
}

fn register_one<F>(conn: &Connection, name: &str, format: F) -> Result<()>
where
    F: Fn(NaiveDate) -> String + Send + Sync + 'static,
{
    conn.create_scalar_function(
        name,
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            // The zone argument is a literal, so SQLite keeps the parsed Tz as
            // auxiliary data for the whole statement instead of re-parsing the
            // zone name on every row.
            let tz = ctx.get_or_create_aux(1, |raw| -> std::result::Result<Tz, String> {
                raw.as_str()
                    .map_err(|err| err.to_string())?
                    .parse::<Tz>()
                    .map_err(|_| "unknown IANA zone".to_string())
            })?;
            let raw = ctx.get_raw(0);
            let Ok(text) = raw.as_str() else {
                return Ok(None);
            };
            let Some(instant) = parse_stored_timestamp(text) else {
                return Ok(None);
            };
            Ok(Some(format(
                instant.with_timezone(tz.as_ref()).date_naive(),
            )))
        },
    )?;
    Ok(())
}

/// Parses a timestamp as stored by the writer.
///
/// Stored values are UTC RFC 3339 (`2026-07-25T13:00:00Z`). That exact shape is
/// parsed from bytes directly because this runs once per scanned row; the
/// chrono parsers are only used for the rarer legacy/SQLite-produced forms.
fn parse_stored_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    if let Some(value) = parse_fast_utc(raw.as_bytes()) {
        return Some(value);
    }
    if let Ok(value) = DateTime::parse_from_rfc3339(raw) {
        return Some(value.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(raw, format) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?));
    }
    None
}

/// Parses `YYYY-MM-DDTHH:MM:SSZ` / `YYYY-MM-DD HH:MM:SS` without going through
/// the general chrono parsers. Returns `None` for anything else.
fn parse_fast_utc(b: &[u8]) -> Option<DateTime<Utc>> {
    if b.len() < 19 {
        return None;
    }
    if b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    if !matches!(b[10], b'T' | b' ') {
        return None;
    }
    // Anything after the seconds field must be UTC-designating (`Z`) or a
    // fractional part; a real offset means the slow path has to handle it.
    if b.len() > 19 && !matches!(b[19], b'Z' | b'z' | b'.') {
        return None;
    }
    if b.len() > 19 && b[19] == b'.' && !b.ends_with(b"Z") && !b.ends_with(b"z") {
        return None;
    }
    let num = |s: &[u8]| -> Option<u32> {
        let mut acc = 0u32;
        for &c in s {
            if !c.is_ascii_digit() {
                return None;
            }
            acc = acc * 10 + u32::from(c - b'0');
        }
        Some(acc)
    };
    let date = NaiveDate::from_ymd_opt(num(&b[0..4])? as i32, num(&b[5..7])?, num(&b[8..10])?)?;
    let time = date.and_hms_opt(num(&b[11..13])?, num(&b[14..16])?, num(&b[17..19])?)?;
    Some(Utc.from_utc_datetime(&time))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

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
