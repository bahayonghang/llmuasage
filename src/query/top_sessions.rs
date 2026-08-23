use std::{cmp::Ordering, collections::HashMap};

use chrono::DateTime;
use rusqlite::{params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::{Dashboard, QueryFilter};

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;
const ACTIVE_GAP_CAP_MINUTES: i64 = 30;

/// Server-side ordering for [`Dashboard::top_sessions`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TopSessionsSort {
    #[default]
    Tokens,
    Duration,
    Cost,
}

impl TopSessionsSort {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "tokens" => Some(Self::Tokens),
            "duration" => Some(Self::Duration),
            "cost" => Some(Self::Cost),
            _ => None,
        }
    }
}

/// Query object for the Top Sessions ranking.
#[derive(Debug, Clone)]
pub struct TopSessionsQuery {
    pub filter: QueryFilter,
    pub sort: TopSessionsSort,
    /// Requested row count. `0` uses 10; all values are clamped to `1..=50`.
    pub limit: u32,
}

impl Default for TopSessionsQuery {
    fn default() -> Self {
        Self {
            filter: QueryFilter::default(),
            sort: TopSessionsSort::Tokens,
            limit: DEFAULT_LIMIT,
        }
    }
}

/// One ranked session aggregate.
///
/// Session identity deliberately matches `reports::event_session_id`: a
/// trimmed non-empty source session id wins, then source path hash, then the
/// source-specific event-key fallback. Source prefixes prevent equal ids from
/// unrelated tools from merging. The Logs API keeps exposing the underlying
/// nullable source session id; this canonical id is the ranking/group key.
#[derive(Debug, Clone, Serialize)]
pub struct TopSessionRow {
    pub session_id: String,
    pub session_label: Option<String>,
    pub project_label: Option<String>,
    pub source: Option<String>,
    /// First event in the current filtered range, serialized as RFC3339 UTC.
    pub first_event_at: String,
    /// Last event in the current filtered range, serialized as RFC3339 UTC.
    pub last_event_at: String,
    pub total_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub span_minutes: i64,
    pub active_minutes: i64,
    pub event_count: i64,
}

pub(crate) fn load(dashboard: &Dashboard, query: &TopSessionsQuery) -> Result<Vec<TopSessionRow>> {
    let limit = normalize_limit(query.limit);
    // Wall-clock span is only an upper bound for active duration. A fixed
    // span-ranked candidate window can therefore omit the real active-time
    // leaders when long idle sessions occupy that window.
    let candidate_limit = (query.sort != TopSessionsSort::Duration).then_some(limit);
    let identity = session_identity_sql("e");
    let filter = query.filter.event_filter(Some("e"));
    let order = match query.sort {
        TopSessionsSort::Tokens => "total_tokens DESC",
        TopSessionsSort::Duration => "canonical_session_id ASC",
        TopSessionsSort::Cost => "cost_usd DESC",
    };
    let limit_clause = candidate_limit.map_or("", |_| "LIMIT ?");
    let sql = format!(
        r#"
        SELECT
            {identity} AS canonical_session_id,
            MIN(NULLIF(e.session_label, '')) AS session_label,
            MIN(NULLIF(e.project_label, '')) AS project_label,
            CASE WHEN MIN(e.source) = MAX(e.source) THEN MIN(e.source) ELSE NULL END AS source,
            COALESCE(SUM(e.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(e.output_tokens + e.reasoning_output_tokens), 0) AS output_tokens,
            COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost_usd,
            COUNT(*) AS event_count,
            MIN(e.event_at) AS first_at,
            MAX(e.event_at) AS last_at,
            (julianday(MAX(e.event_at)) - julianday(MIN(e.event_at))) * 1440.0 AS rough_span
        FROM usage_event e
        {}
        GROUP BY canonical_session_id
        ORDER BY {order}, canonical_session_id ASC
        {limit_clause}
        "#,
        filter.where_sql()
    );
    let mut params = filter.into_params();
    if let Some(candidate_limit) = candidate_limit {
        params.push(SqlValue::Integer(candidate_limit as i64));
    }
    let mut stmt = dashboard.conn.prepare(&sql)?;
    let candidates = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(TopSessionRow {
                session_id: row.get(0)?,
                session_label: row.get(1)?,
                project_label: row.get(2)?,
                source: row.get(3)?,
                total_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                cost_usd: row.get(6)?,
                span_minutes: 0,
                active_minutes: 0,
                event_count: row.get(7)?,
                first_event_at: row.get(8)?,
                last_event_at: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let all_times = if query.sort == TopSessionsSort::Duration {
        Some(all_session_event_times(dashboard, &query.filter)?)
    } else {
        None
    };
    let mut rows = Vec::with_capacity(candidates.len());
    for mut row in candidates {
        let owned_times;
        let times: &[String] = if let Some(all_times) = &all_times {
            all_times
                .get(&row.session_id)
                .map(Vec::as_slice)
                .unwrap_or_default()
        } else {
            owned_times = session_event_times(dashboard, &query.filter, &row.session_id)?;
            &owned_times
        };
        let (span, active) = session_time_span(times, &row.first_event_at, &row.last_event_at);
        row.span_minutes = span;
        row.active_minutes = active;
        rows.push(row);
    }

    rows.sort_by(|a, b| compare_rows(a, b, query.sort));
    rows.truncate(limit as usize);
    Ok(rows)
}

fn all_session_event_times(
    dashboard: &Dashboard,
    filter: &QueryFilter,
) -> Result<HashMap<String, Vec<String>>> {
    let identity = session_identity_sql("e");
    let filter = filter.event_filter(Some("e"));
    let sql = format!(
        "SELECT {identity}, e.event_at FROM usage_event e{} ORDER BY 1 ASC, e.event_at ASC",
        filter.where_sql()
    );
    let mut stmt = dashboard.conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(filter.params().iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut grouped = HashMap::<String, Vec<String>>::new();
    for row in rows {
        let (session_id, event_at) = row?;
        grouped.entry(session_id).or_default().push(event_at);
    }
    Ok(grouped)
}

fn session_event_times(
    dashboard: &Dashboard,
    filter: &QueryFilter,
    session_id: &str,
) -> Result<Vec<String>> {
    let identity = session_identity_sql("e");
    let mut filter = filter.event_filter(Some("e"));
    filter.push(format!("({identity}) = ?"), session_id.to_string());
    let sql = format!(
        "SELECT e.event_at FROM usage_event e{} ORDER BY e.event_at ASC",
        filter.where_sql()
    );
    let mut stmt = dashboard.conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(filter.params().iter()), |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn session_time_span(times: &[String], first_at: &str, last_at: &str) -> (i64, i64) {
    let parse = |raw: &str| DateTime::parse_from_rfc3339(raw).ok();
    let span = parse(last_at)
        .zip(parse(first_at))
        .map(|(last, first)| (last - first).num_minutes().max(0))
        .unwrap_or(0);
    let active = times
        .windows(2)
        .filter_map(|pair| parse(&pair[1]).zip(parse(&pair[0])))
        .map(|(next, previous)| (next - previous).num_minutes())
        .filter(|gap| *gap > 0 && *gap <= ACTIVE_GAP_CAP_MINUTES)
        .sum();
    (span, active)
}

fn compare_rows(a: &TopSessionRow, b: &TopSessionRow, sort: TopSessionsSort) -> Ordering {
    let primary = match sort {
        TopSessionsSort::Tokens => b.total_tokens.cmp(&a.total_tokens),
        TopSessionsSort::Duration => b.active_minutes.cmp(&a.active_minutes),
        TopSessionsSort::Cost => b.cost_usd.total_cmp(&a.cost_usd),
    };
    primary.then_with(|| a.session_id.cmp(&b.session_id))
}

fn normalize_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.clamp(1, MAX_LIMIT)
    }
}

pub(super) fn session_identity_sql(alias: &str) -> String {
    let event_key = format!("{alias}.event_key");
    let source = format!("{alias}.source");
    let tail = format!("substr({event_key}, instr({event_key}, ':') + 1)");
    let second = format!("instr({tail}, ':')");
    let after_second = format!("substr({tail}, {second} + 1)");
    let third = format!("instr({after_second}, ':')");
    format!(
        "CASE \
         WHEN trim(COALESCE({alias}.session_id, '')) <> '' \
           THEN {source} || ':' || trim({alias}.session_id) \
         WHEN {alias}.source_path_hash IS NOT NULL \
           THEN {source} || ':' || {alias}.source_path_hash \
         WHEN {source} IN ('codex', 'claude') AND {second} > 0 AND {third} > 0 \
           THEN {source} || ':' || substr({tail}, 1, {second} + {third} - 1) \
         ELSE {event_key} END"
    )
}

#[cfg(test)]
mod tests {
    use super::{TopSessionsQuery, TopSessionsSort, normalize_limit};

    #[test]
    fn limit_defaults_and_clamps() {
        assert_eq!(normalize_limit(0), 10);
        assert_eq!(normalize_limit(1), 1);
        assert_eq!(normalize_limit(500), 50);
    }

    #[test]
    fn sort_parser_is_strict() {
        assert_eq!(
            TopSessionsSort::parse("duration"),
            Some(TopSessionsSort::Duration)
        );
        assert!(TopSessionsSort::parse("messages").is_none());
        assert_eq!(TopSessionsQuery::default().limit, 10);
    }
}
