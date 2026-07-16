use chrono::{
    Duration, FixedOffset, Local, NaiveDate, Offset, SecondsFormat, TimeZone, Utc,
    offset::LocalResult,
};
use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use super::{Dashboard, QueryFilter, ReportTimezone};
use crate::error::Result;

/// One day on the activity heatmap (F4.3).
///
/// `event_count` and `total_tokens` are summed from `usage_bucket_30m`
/// using [`QueryFilter::timezone`] to fold UTC `hour_start` rows into
/// local calendar dates. Days without activity are zero-filled so
/// callers can render a continuous grid.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HeatmapPoint {
    /// Local calendar date in `YYYY-MM-DD`.
    pub date: String,
    /// Number of usage events that landed on this local date.
    pub event_count: i64,
    /// Total tokens accumulated on this local date.
    pub total_tokens: i64,
}

const MAX_DAYS: u32 = 366;

pub(super) fn load(
    dashboard: &Dashboard,
    filter: &QueryFilter,
    days: u32,
) -> Result<Vec<HeatmapPoint>> {
    let window = days.clamp(1, MAX_DAYS);
    let today = today_in(&filter.timezone);
    let earliest = today
        .checked_sub_signed(Duration::days((window - 1) as i64))
        .unwrap_or(today);

    let observed = load_observed(&dashboard.conn, filter, &earliest)?;

    Ok((0..window)
        .map(|offset| {
            let date = earliest + Duration::days(offset as i64);
            let key = date.format("%Y-%m-%d").to_string();
            let (event_count, total_tokens) = observed
                .iter()
                .find(|(captured_date, _, _)| captured_date == &key)
                .map(|(_, events, tokens)| (*events, *tokens))
                .unwrap_or_default();
            HeatmapPoint {
                date: key,
                event_count,
                total_tokens,
            }
        })
        .collect())
}

fn load_observed(
    conn: &Connection,
    filter: &QueryFilter,
    earliest_local: &NaiveDate,
) -> Result<Vec<(String, i64, i64)>> {
    let mut sql_filter = filter.bucket_filter(None);
    let earliest_local_start = earliest_local
        .and_hms_opt(0, 0, 0)
        .expect("midnight always valid");
    let offset = fixed_offset_for(&filter.timezone);
    let earliest_utc = match offset.from_local_datetime(&earliest_local_start) {
        LocalResult::Single(value) => value.with_timezone(&Utc),
        LocalResult::Ambiguous(earliest, _) => earliest.with_timezone(&Utc),
        LocalResult::None => offset
            .from_utc_datetime(&earliest_local_start)
            .with_timezone(&Utc),
    };
    sql_filter.push(
        "hour_start >= ?",
        earliest_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
    );

    let modifier = filter.local_time_modifier();
    let sql = format!(
        r#"
        SELECT
            date(hour_start, '{modifier}') AS local_date,
            COALESCE(SUM(event_count), 0),
            COALESCE(SUM(total_tokens), 0)
        FROM usage_bucket_30m
        {}
        GROUP BY local_date
        ORDER BY local_date ASC
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn today_in(timezone: &ReportTimezone) -> NaiveDate {
    let utc = Utc::now();
    match timezone {
        ReportTimezone::Utc => utc.date_naive(),
        ReportTimezone::Local => Local::now().date_naive(),
        ReportTimezone::Fixed(offset) => utc.with_timezone(offset).date_naive(),
    }
}

fn fixed_offset_for(timezone: &ReportTimezone) -> FixedOffset {
    match timezone {
        ReportTimezone::Utc => Utc.fix(),
        ReportTimezone::Local => Local::now().offset().fix(),
        ReportTimezone::Fixed(offset) => *offset,
    }
}
