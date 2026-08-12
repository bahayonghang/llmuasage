use chrono::{DateTime, Datelike, Timelike, Utc};
use rusqlite::params_from_iter;
use serde::Serialize;

use crate::error::Result;

use super::{Dashboard, QueryFilter, timezone::ResolvedZone};

/// One cell in a zero-filled 7x24 local-time activity grid.
///
/// `dow` uses Monday=0 through Sunday=6. Each persisted 30-minute bucket is
/// assigned by the local weekday/hour containing its UTC `hour_start` instant.
#[derive(Debug, Clone, Serialize)]
pub struct HourOfWeekCell {
    pub dow: u8,
    pub hour: u8,
    pub total_tokens: i64,
    pub event_count: i64,
}

pub(crate) fn load(dashboard: &Dashboard, filter: &QueryFilter) -> Result<Vec<HourOfWeekCell>> {
    let sql_filter = filter.bucket_filter(Some("b"));
    let sql = format!(
        r#"
        SELECT b.hour_start, COALESCE(SUM(b.total_tokens), 0), COALESCE(SUM(b.event_count), 0)
        FROM usage_bucket_30m b
        {}
        GROUP BY b.hour_start
        ORDER BY b.hour_start ASC
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = dashboard.conn.prepare(&sql)?;
    let buckets = stmt
        .query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let zone = filter.timezone.resolved();
    let mut values = vec![(0i64, 0i64); 7 * 24];
    for (hour_start, tokens, events) in buckets {
        let Ok(instant) = DateTime::parse_from_rfc3339(&hour_start) else {
            continue;
        };
        let (dow, hour) = local_weekday_hour(&zone, instant.with_timezone(&Utc));
        let cell = &mut values[dow as usize * 24 + hour as usize];
        cell.0 += tokens;
        cell.1 += events;
    }

    Ok(values
        .into_iter()
        .enumerate()
        .map(|(index, (total_tokens, event_count))| HourOfWeekCell {
            dow: (index / 24) as u8,
            hour: (index % 24) as u8,
            total_tokens,
            event_count,
        })
        .collect())
}

fn local_weekday_hour(zone: &ResolvedZone, instant: DateTime<Utc>) -> (u8, u8) {
    match zone {
        ResolvedZone::Fixed(offset) => {
            let local = instant.with_timezone(offset);
            (
                local.weekday().num_days_from_monday() as u8,
                local.hour() as u8,
            )
        }
        ResolvedZone::Iana(tz) => {
            let local = instant.with_timezone(tz);
            (
                local.weekday().num_days_from_monday() as u8,
                local.hour() as u8,
            )
        }
    }
}
