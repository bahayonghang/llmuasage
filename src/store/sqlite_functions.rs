//! DST-aware SQLite scalar functions for local date/hour grouping.
//!
//! Query SQL builders emit these function names. Every connection that may run
//! a `Local`-timezone query must register them.

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use rusqlite::{Connection, functions::FunctionFlags};

use crate::error::Result;

/// SQL function name for DST-aware local date extraction.
pub(crate) const FN_LOCAL_DATE: &str = "llmusage_local_date";
/// SQL function name for DST-aware local `YYYY-MM` extraction.
pub(crate) const FN_LOCAL_MONTH: &str = "llmusage_local_month";
/// SQL function name for DST-aware local `YYYY-WW` extraction.
pub(crate) const FN_LOCAL_WEEK: &str = "llmusage_local_week";
/// SQL function name for DST-aware local `YYYY-MM-DD HH:00` extraction.
pub(crate) const FN_LOCAL_HOUR: &str = "llmusage_local_hour";

/// Registers the DST-aware date/month functions on a connection.
///
/// Must be called on every connection that may run a `Local`-timezone query.
/// The functions are deterministic for a given (timestamp, zone) pair, so
/// SQLite is free to cache and index-optimize them.
pub(crate) fn register_functions(conn: &Connection) -> Result<()> {
    register_one(conn, FN_LOCAL_DATE, format_local_date)?;
    register_one(conn, FN_LOCAL_MONTH, format_local_month)?;
    register_one(conn, FN_LOCAL_WEEK, format_local_week)?;
    register_hour(conn)?;
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

fn format_local_hour(local: NaiveDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:00",
        local.year(),
        local.month(),
        local.day(),
        local.hour()
    )
}

/// `%Y-%W` matching C / SQLite `strftime` semantics.
///
/// `%W` is the week number with the first Monday as day 1 of week 1 (00-53).
/// This must agree exactly with the SQLite `strftime('%Y-%W', ...)` used on the
/// fixed-offset path, otherwise the two timezone modes would bucket weeks
/// differently.
pub(crate) fn format_local_week(date: NaiveDate) -> String {
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

fn register_hour(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        FN_LOCAL_HOUR,
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
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
            Ok(Some(format_local_hour(
                instant.with_timezone(tz.as_ref()).naive_local(),
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
pub(crate) fn parse_fast_utc(b: &[u8]) -> Option<DateTime<Utc>> {
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
