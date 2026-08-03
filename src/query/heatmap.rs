use chrono::{Duration, NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};

use super::{Dashboard, QueryFilter, ReportTimezone};
use crate::error::Result;

/// One day on the activity heatmap (F4.3).
///
/// `event_count` and `total_tokens` are summed from `usage_bucket_30m`
/// using [`QueryFilter::timezone`] to fold UTC `hour_start` rows into
/// local calendar dates. Days without activity are zero-filled so
/// callers can render a continuous grid.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    let window_end = filter.until.unwrap_or_else(|| today_in(&filter.timezone));
    let earliest = window_end
        .checked_sub_signed(Duration::days((window - 1) as i64))
        .unwrap_or(window_end);

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
    // DATA-003: resolve the day boundary with real tz rules, not one snapshot
    // offset, so the lower bound is correct across a DST transition.
    let earliest_utc = filter
        .timezone
        .resolved()
        .local_date_start_utc(*earliest_local);
    sql_filter.push(
        "hour_start >= ?",
        earliest_utc.to_rfc3339_opts(SecondsFormat::Secs, true),
    );

    let local_date = filter.local_date_expr("hour_start");
    let sql = format!(
        r#"
        SELECT
            {local_date} AS local_date,
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
    timezone.date_at(Utc::now())
}
