use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

use chrono::{DateTime, FixedOffset};
use rusqlite::params_from_iter;
use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::{Dashboard, QueryFilter};

const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;
const ACTIVE_GAP_CAP_MINUTES: i64 = 30;
const TOP_SESSIONS_COVER_INDEX: &str = "idx_usage_event_top_sessions_cover";
const TOP_SESSIONS_UNHINTED_FROM: &str = "usage_event AS e";

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

struct ProjectedEvent {
    session_label: Option<String>,
    project_label: Option<String>,
    source: String,
    total_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
    cost_usd: Option<f64>,
    event_at: String,
}

struct SessionAccumulator {
    session_label: Option<String>,
    project_label: Option<String>,
    source: Option<String>,
    total_tokens: i64,
    output_tokens: i64,
    cost_usd: SqliteFloatSum,
    event_times: Vec<String>,
    event_count: i64,
}

impl SessionAccumulator {
    fn new(event: ProjectedEvent) -> rusqlite::Result<Self> {
        let mut cost_usd = SqliteFloatSum::default();
        if let Some(cost) = event.cost_usd {
            cost_usd.add(cost);
        }
        let output_tokens = event
            .output_tokens
            .checked_add(event.reasoning_output_tokens)
            .ok_or(rusqlite::Error::IntegralValueOutOfRange(
                6,
                event.reasoning_output_tokens,
            ))?;
        Ok(Self {
            session_label: non_empty(event.session_label),
            project_label: non_empty(event.project_label),
            source: Some(event.source),
            total_tokens: event.total_tokens,
            output_tokens,
            cost_usd,
            event_times: vec![event.event_at],
            event_count: 1,
        })
    }

    fn add(&mut self, event: ProjectedEvent) -> rusqlite::Result<()> {
        update_min(&mut self.session_label, event.session_label);
        update_min(&mut self.project_label, event.project_label);
        if self.source.as_deref() != Some(event.source.as_str()) {
            self.source = None;
        }
        self.total_tokens = self.total_tokens.checked_add(event.total_tokens).ok_or(
            rusqlite::Error::IntegralValueOutOfRange(4, event.total_tokens),
        )?;
        let output_tokens = event
            .output_tokens
            .checked_add(event.reasoning_output_tokens)
            .ok_or(rusqlite::Error::IntegralValueOutOfRange(
                6,
                event.reasoning_output_tokens,
            ))?;
        self.output_tokens = self
            .output_tokens
            .checked_add(output_tokens)
            .ok_or(rusqlite::Error::IntegralValueOutOfRange(5, output_tokens))?;
        if let Some(cost) = event.cost_usd {
            self.cost_usd.add(cost);
        }
        self.event_times.push(event.event_at);
        self.event_count += 1;
        Ok(())
    }

    fn finish(mut self, session_id: String) -> TopSessionRow {
        self.event_times.sort_unstable();
        let (span_minutes, active_minutes) = session_time_span(&self.event_times);
        let mut event_times = self.event_times.into_iter();
        let first_event_at = event_times
            .next()
            .expect("a session accumulator always contains one event");
        let last_event_at = event_times
            .next_back()
            .unwrap_or_else(|| first_event_at.clone());
        TopSessionRow {
            session_id,
            session_label: self.session_label,
            project_label: self.project_label,
            source: self.source,
            first_event_at,
            last_event_at,
            total_tokens: self.total_tokens,
            output_tokens: self.output_tokens,
            cost_usd: self.cost_usd.finish(),
            span_minutes,
            active_minutes,
            event_count: self.event_count,
        }
    }
}

/// Mirrors SQLite's compensated floating-point `SUM()` implementation.
///
/// Top Sessions historically accumulated costs inside SQLite. Keeping the
/// same Kahan-Babuska-Neumaier step preserves serialized `cost_usd` bytes when
/// the reducer moves into Rust, including cancellation-heavy value sequences.
#[derive(Default)]
struct SqliteFloatSum {
    sum: f64,
    error: f64,
    count: usize,
}

impl SqliteFloatSum {
    fn add(&mut self, value: f64) {
        let sum = self.sum;
        let next = sum + value;
        if sum.abs() > value.abs() {
            self.error += (sum - next) + value;
        } else {
            self.error += (value - next) + sum;
        }
        self.sum = next;
        self.count += 1;
    }

    fn finish(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else if self.error.is_finite() {
            self.sum + self.error
        } else {
            self.sum
        }
    }
}

pub(crate) fn load(dashboard: &Dashboard, query: &TopSessionsQuery) -> Result<Vec<TopSessionRow>> {
    let limit = normalize_limit(query.limit) as usize;
    let (sql, filter) = projection_sql(query);
    let mut stmt = dashboard.conn.prepare(&sql)?;
    let events = stmt.query_map(params_from_iter(filter.params().iter()), |row| {
        Ok((
            row.get(0)?,
            ProjectedEvent {
                session_label: row.get(1)?,
                project_label: row.get(2)?,
                source: row.get(3)?,
                total_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                reasoning_output_tokens: row.get(6)?,
                cost_usd: row.get(7)?,
                event_at: row.get(8)?,
            },
        ))
    })?;

    let mut sessions = HashMap::<String, SessionAccumulator>::new();
    for event in events {
        let (session_id, event) = event?;
        match sessions.entry(session_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => entry.get_mut().add(event)?,
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(SessionAccumulator::new(event)?);
            }
        }
    }

    if query.sort == TopSessionsSort::Duration {
        let rows = sessions
            .into_iter()
            .map(|(session_id, accumulator)| accumulator.finish(session_id));
        Ok(select_top_rows(rows, limit, query.sort))
    } else {
        Ok(select_top_accumulators(sessions, limit, query.sort)
            .into_iter()
            .map(|(session_id, accumulator)| accumulator.finish(session_id))
            .collect())
    }
}

fn projection_sql(query: &TopSessionsQuery) -> (String, super::filter::SqlFilter) {
    let identity = session_identity_sql("e");
    let filter = query.filter.event_filter(Some("e"));
    let from = if query.filter.since.is_none() && query.filter.until.is_none() {
        format!("usage_event AS e INDEXED BY {TOP_SESSIONS_COVER_INDEX}")
    } else {
        TOP_SESSIONS_UNHINTED_FROM.to_string()
    };
    let sql = format!(
        r#"
        /* top_sessions_projection */
        SELECT
            {identity} AS canonical_session_id,
            NULLIF(e.session_label, '') AS session_label,
            NULLIF(e.project_label, '') AS project_label,
            e.source,
            e.total_tokens,
            e.output_tokens,
            e.reasoning_output_tokens,
            e.cost_with_cache_usd,
            e.event_at
        FROM {from}
        {}
        "#,
        filter.where_sql()
    );
    (sql, filter)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn update_min(current: &mut Option<String>, candidate: Option<String>) {
    let Some(candidate) = non_empty(candidate) else {
        return;
    };
    if current.as_ref().is_none_or(|current| candidate < *current) {
        *current = Some(candidate);
    }
}

struct RankedRow {
    row: TopSessionRow,
    sort: TopSessionsSort,
}

struct RankedAccumulator {
    session_id: String,
    accumulator: SessionAccumulator,
    sort: TopSessionsSort,
}

impl PartialEq for RankedRow {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedRow {}

impl PartialOrd for RankedRow {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedRow {
    fn cmp(&self, other: &Self) -> Ordering {
        debug_assert_eq!(self.sort, other.sort);
        compare_rows(&self.row, &other.row, self.sort)
    }
}

impl PartialEq for RankedAccumulator {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedAccumulator {}

impl PartialOrd for RankedAccumulator {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedAccumulator {
    fn cmp(&self, other: &Self) -> Ordering {
        debug_assert_eq!(self.sort, other.sort);
        compare_rankings(
            &self.session_id,
            self.accumulator.ranking_metrics(),
            &other.session_id,
            other.accumulator.ranking_metrics(),
            self.sort,
        )
    }
}

fn select_top_rows(
    rows: impl IntoIterator<Item = TopSessionRow>,
    limit: usize,
    sort: TopSessionsSort,
) -> Vec<TopSessionRow> {
    let mut top = BinaryHeap::<RankedRow>::with_capacity(limit.saturating_add(1));
    for row in rows {
        let should_insert = if top.len() < limit {
            true
        } else {
            top.peek()
                .is_some_and(|worst| compare_rows(&row, &worst.row, sort).is_lt())
        };
        if should_insert {
            if top.len() == limit {
                top.pop();
            }
            top.push(RankedRow { row, sort });
        }
    }
    let mut rows = top.into_iter().map(|ranked| ranked.row).collect::<Vec<_>>();
    rows.sort_by(|a, b| compare_rows(a, b, sort));
    rows
}

fn select_top_accumulators(
    sessions: HashMap<String, SessionAccumulator>,
    limit: usize,
    sort: TopSessionsSort,
) -> Vec<(String, SessionAccumulator)> {
    debug_assert_ne!(sort, TopSessionsSort::Duration);
    let mut top = BinaryHeap::<RankedAccumulator>::with_capacity(limit.saturating_add(1));
    for (session_id, accumulator) in sessions {
        let should_insert = if top.len() < limit {
            true
        } else {
            top.peek().is_some_and(|worst| {
                compare_rankings(
                    &session_id,
                    accumulator.ranking_metrics(),
                    &worst.session_id,
                    worst.accumulator.ranking_metrics(),
                    sort,
                )
                .is_lt()
            })
        };
        if should_insert {
            if top.len() == limit {
                top.pop();
            }
            top.push(RankedAccumulator {
                session_id,
                accumulator,
                sort,
            });
        }
    }
    let mut sessions = top
        .into_iter()
        .map(|ranked| (ranked.session_id, ranked.accumulator))
        .collect::<Vec<_>>();
    sessions.sort_by(|(a_id, a), (b_id, b)| {
        compare_rankings(a_id, a.ranking_metrics(), b_id, b.ranking_metrics(), sort)
    });
    sessions
}

fn session_time_span(times: &[String]) -> (i64, i64) {
    session_time_span_with(times, |raw| DateTime::parse_from_rfc3339(raw).ok())
}

fn session_time_span_with(
    times: &[String],
    mut parse: impl FnMut(&str) -> Option<DateTime<FixedOffset>>,
) -> (i64, i64) {
    let first = times.first().and_then(|raw| parse(raw));
    let mut previous = first;
    let mut active = 0;
    for raw in times.iter().skip(1) {
        let current = parse(raw);
        if let (Some(next), Some(previous)) = (current.as_ref(), previous.as_ref()) {
            let gap = (*next - *previous).num_minutes();
            if gap > 0 && gap <= ACTIVE_GAP_CAP_MINUTES {
                active += gap;
            }
        }
        previous = current;
    }
    let span = previous
        .zip(first)
        .map(|(last, first)| (last - first).num_minutes().max(0))
        .unwrap_or(0);
    (span, active)
}

fn compare_rows(a: &TopSessionRow, b: &TopSessionRow, sort: TopSessionsSort) -> Ordering {
    compare_rankings(
        &a.session_id,
        RankingMetrics {
            total_tokens: a.total_tokens,
            active_minutes: a.active_minutes,
            cost_usd: a.cost_usd,
        },
        &b.session_id,
        RankingMetrics {
            total_tokens: b.total_tokens,
            active_minutes: b.active_minutes,
            cost_usd: b.cost_usd,
        },
        sort,
    )
}

#[derive(Clone, Copy)]
struct RankingMetrics {
    total_tokens: i64,
    active_minutes: i64,
    cost_usd: f64,
}

impl SessionAccumulator {
    fn ranking_metrics(&self) -> RankingMetrics {
        RankingMetrics {
            total_tokens: self.total_tokens,
            active_minutes: 0,
            cost_usd: self.cost_usd.finish(),
        }
    }
}

fn compare_rankings(
    a_id: &str,
    a: RankingMetrics,
    b_id: &str,
    b: RankingMetrics,
    sort: TopSessionsSort,
) -> Ordering {
    let primary = match sort {
        TopSessionsSort::Tokens => b.total_tokens.cmp(&a.total_tokens),
        TopSessionsSort::Duration => b.active_minutes.cmp(&a.active_minutes),
        TopSessionsSort::Cost => b.cost_usd.total_cmp(&a.cost_usd),
    };
    primary.then_with(|| a_id.cmp(b_id))
}

fn normalize_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.clamp(1, MAX_LIMIT)
    }
}

pub(super) fn session_identity_sql(alias: &str) -> String {
    let column = |name: &str| {
        if alias.is_empty() {
            name.to_string()
        } else {
            format!("{alias}.{name}")
        }
    };
    let event_key = column("event_key");
    let source = column("source");
    let session_id = column("session_id");
    let source_path_hash = column("source_path_hash");
    let tail = format!("substr({event_key}, instr({event_key}, ':') + 1)");
    let second = format!("instr({tail}, ':')");
    let after_second = format!("substr({tail}, {second} + 1)");
    let third = format!("instr({after_second}, ':')");
    format!(
        "CASE \
         WHEN trim(COALESCE({session_id}, '')) <> '' \
           THEN {source} || ':' || trim({session_id}) \
         WHEN {source_path_hash} IS NOT NULL \
           THEN {source} || ':' || {source_path_hash} \
         WHEN {source} IN ('codex', 'claude') AND {second} > 0 AND {third} > 0 \
           THEN {source} || ':' || substr({tail}, 1, {second} + {third} - 1) \
         ELSE {event_key} END"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    use chrono::{FixedOffset, NaiveDate};
    use rusqlite::{params_from_iter, types::Value as SqlValue};
    use tempfile::TempDir;

    use crate::{AppPaths, Store, models::SourceKind};

    use super::*;

    static TOP_SESSIONS_EVENT_STATEMENTS: AtomicUsize = AtomicUsize::new(0);

    fn count_top_sessions_event_statements(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event
            && sql.contains("/* top_sessions_projection */")
        {
            TOP_SESSIONS_EVENT_STATEMENTS.fetch_add(1, AtomicOrdering::Relaxed);
        }
    }

    fn fixture() -> Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    fn projection_plan(
        dashboard: &Dashboard,
        query: &TopSessionsQuery,
    ) -> Result<(String, String)> {
        let (sql, filter) = projection_sql(query);
        let explain = format!("EXPLAIN QUERY PLAN {sql}");
        let plan = dashboard
            .conn
            .prepare(&explain)?
            .query_map(params_from_iter(filter.params().iter()), |row| {
                row.get::<_, String>(3)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .join("\n");
        Ok((sql, plan))
    }

    fn compact_sql(sql: &str) -> String {
        sql.chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect()
    }

    fn exact_top_sessions_cover_index_sql() -> String {
        let identity = session_identity_sql("");
        format!(
            "CREATE INDEX {TOP_SESSIONS_COVER_INDEX} ON usage_event(({identity}), event_at, session_label, project_label, source, total_tokens, output_tokens, reasoning_output_tokens, cost_with_cache_usd, model, project_hash, host_id)"
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_event(
        store: &Store,
        event_key: &str,
        source: &str,
        model: &str,
        event_at: &str,
        session_id: Option<&str>,
        source_path_hash: Option<&str>,
        session_label: Option<&str>,
        project_hash: Option<&str>,
        project_label: Option<&str>,
        host_id: Option<&str>,
        total_tokens: i64,
        output_tokens: i64,
        reasoning_output_tokens: i64,
        cost_usd: f64,
    ) -> Result<()> {
        store.open_connection()?.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_creation_tokens, cache_read_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, project_label, path_hash, session_id, session_label,
                source_path_hash, host_id, created_at, cost_with_cache_usd,
                cost_without_cache_usd, pricing_status
            ) VALUES (?1, ?2, ?3, ?4, ?4, 0, 0, 0, ?12, ?13, ?11,
                      ?8, ?9, ?1, ?5, ?7, ?6, COALESCE(?10, ''), ?4, ?14, ?14, 'static')
            "#,
            rusqlite::params![
                event_key,
                source,
                model,
                event_at,
                session_id,
                source_path_hash,
                session_label,
                project_hash,
                project_label,
                host_id,
                total_tokens,
                output_tokens,
                reasoning_output_tokens,
                cost_usd,
            ],
        )?;
        Ok(())
    }

    fn seed_oracle_fixture(store: &Store) -> Result<()> {
        for event in [
            (
                "explicit-2",
                "codex",
                "gpt-a",
                "2026-05-02T00:30:00Z",
                Some("  explicit  "),
                Some("ignored-path"),
                Some("Zulu"),
                Some("project-a"),
                Some("Project Z"),
                Some("host-a"),
                7,
                2,
                3,
                1.0,
            ),
            (
                "explicit-1",
                "codex",
                "gpt-a",
                "2026-05-02T00:00:00Z",
                Some("explicit"),
                None,
                Some("Alpha"),
                Some("project-a"),
                Some("Project A"),
                Some("host-a"),
                5,
                1,
                1,
                1.0e16,
            ),
            (
                "explicit-3",
                "codex",
                "gpt-a",
                "2026-05-02T01:31:00Z",
                Some("explicit"),
                None,
                None,
                Some("project-a"),
                None,
                Some("host-a"),
                9,
                4,
                0,
                -1.0e16,
            ),
            (
                "path-event",
                "claude",
                "gpt-b",
                "2026-05-03T00:00:00Z",
                Some("  "),
                Some("path-fallback"),
                Some("Path label"),
                Some("project-b"),
                Some("Project B"),
                Some("host-b"),
                21,
                5,
                2,
                2.5,
            ),
            (
                "codex:thread-x:turn-y:event-1",
                "codex",
                "gpt-c",
                "2026-05-04T00:00:00Z",
                None,
                None,
                None,
                None,
                None,
                None,
                21,
                6,
                0,
                2.5,
            ),
            (
                "claude:thread-x:turn-y:event-2",
                "claude",
                "gpt-b",
                "2026-05-04T00:10:00Z",
                None,
                None,
                Some("Claude fallback"),
                Some("project-b"),
                Some("Project B"),
                Some("host-b"),
                3,
                1,
                0,
                0.0,
            ),
            (
                "opencode:event:fallback",
                "opencode",
                "gpt-d",
                "2026-05-05T00:00:00Z",
                None,
                None,
                Some("Event fallback"),
                None,
                None,
                Some("host-c"),
                0,
                0,
                0,
                0.0,
            ),
            (
                "empty-path",
                "codex",
                "gpt-e",
                "2026-05-06T00:00:00Z",
                None,
                Some(""),
                Some("Empty path"),
                None,
                None,
                Some("host-c"),
                1,
                0,
                0,
                0.0,
            ),
        ] {
            insert_event(
                store, event.0, event.1, event.2, event.3, event.4, event.5, event.6, event.7,
                event.8, event.9, event.10, event.11, event.12, event.13,
            )?;
        }
        Ok(())
    }

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

    #[test]
    fn single_projection_matches_legacy_for_empty_store() -> Result<()> {
        let (_temp, store) = fixture()?;
        let dashboard = Dashboard::open(&store)?;
        let query = TopSessionsQuery::default();
        dashboard
            .conn
            .execute_batch(&format!("DROP INDEX {TOP_SESSIONS_COVER_INDEX};"))?;
        let legacy =
            serde_json::to_vec(&load_legacy(&dashboard, &query)?).expect("legacy rows serialize");
        dashboard
            .conn
            .execute_batch(&exact_top_sessions_cover_index_sql())?;
        assert_eq!(
            serde_json::to_vec(&load(&dashboard, &query)?).expect("candidate rows serialize"),
            legacy
        );
        Ok(())
    }

    #[test]
    fn single_projection_matches_legacy_serialized_oracle_matrix() -> Result<()> {
        let (_temp, store) = fixture()?;
        seed_oracle_fixture(&store)?;
        let dashboard = Dashboard::open(&store)?;
        let filters = [
            QueryFilter::default(),
            QueryFilter {
                source: Some(SourceKind::Codex),
                ..QueryFilter::default()
            },
            QueryFilter {
                model: Some("gpt-b".to_string()),
                ..QueryFilter::default()
            },
            QueryFilter {
                project_hash: Some("project-a".to_string()),
                ..QueryFilter::default()
            },
            QueryFilter {
                host_id: Some("host-b".to_string()),
                ..QueryFilter::default()
            },
            QueryFilter {
                since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
                until: Some(NaiveDate::from_ymd_opt(2026, 5, 4).unwrap()),
                timezone: super::super::ReportTimezone::Fixed(
                    FixedOffset::east_opt(8 * 3_600).unwrap(),
                ),
                ..QueryFilter::default()
            },
            QueryFilter {
                source: Some(SourceKind::Claude),
                model: Some("gpt-b".to_string()),
                project_hash: Some("project-b".to_string()),
                host_id: Some("host-b".to_string()),
                since: Some(NaiveDate::from_ymd_opt(2026, 5, 3).unwrap()),
                until: Some(NaiveDate::from_ymd_opt(2026, 5, 4).unwrap()),
                timezone: super::super::ReportTimezone::Utc,
            },
        ];

        let mut cases = Vec::new();
        for filter in filters {
            for sort in [
                TopSessionsSort::Tokens,
                TopSessionsSort::Duration,
                TopSessionsSort::Cost,
            ] {
                for limit in [0, 1, 10, 50, 500] {
                    cases.push(TopSessionsQuery {
                        filter: filter.clone(),
                        sort,
                        limit,
                    });
                }
            }
        }

        dashboard
            .conn
            .execute_batch(&format!("DROP INDEX {TOP_SESSIONS_COVER_INDEX};"))?;
        let legacy_oracles = cases
            .iter()
            .map(|query| {
                Ok((
                    query.clone(),
                    serde_json::to_vec(&load_legacy(&dashboard, query)?)
                        .expect("legacy rows serialize"),
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        dashboard
            .conn
            .execute_batch(&exact_top_sessions_cover_index_sql())?;
        for (query, legacy) in legacy_oracles {
            let candidate = load(&dashboard, &query)?;
            assert_eq!(
                serde_json::to_vec(&candidate).expect("candidate rows serialize"),
                legacy,
                "serialized mismatch for sort={:?}, limit={}, filter={:?}",
                query.sort,
                query.limit,
                query.filter,
            );
        }
        Ok(())
    }

    #[test]
    fn single_projection_uses_one_event_statement_for_every_sort_and_limit() -> Result<()> {
        let (_temp, store) = fixture()?;
        seed_oracle_fixture(&store)?;
        let dashboard = Dashboard::open(&store)?;
        dashboard.conn.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(count_top_sessions_event_statements),
        );

        for sort in [
            TopSessionsSort::Tokens,
            TopSessionsSort::Duration,
            TopSessionsSort::Cost,
        ] {
            for limit in [1, 50] {
                TOP_SESSIONS_EVENT_STATEMENTS.store(0, AtomicOrdering::Relaxed);
                load(
                    &dashboard,
                    &TopSessionsQuery {
                        sort,
                        limit,
                        ..TopSessionsQuery::default()
                    },
                )?;
                assert_eq!(
                    TOP_SESSIONS_EVENT_STATEMENTS.load(AtomicOrdering::Relaxed),
                    1,
                    "sort={sort:?}, limit={limit} must execute one event projection"
                );
            }
        }
        dashboard
            .conn
            .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
        Ok(())
    }

    #[test]
    fn unbounded_projection_shapes_force_the_exact_covering_index() -> Result<()> {
        let (_temp, store) = fixture()?;
        let dashboard = Dashboard::open(&store)?;
        let shapes = [
            ("all", QueryFilter::default()),
            (
                "source",
                QueryFilter {
                    source: Some(SourceKind::Codex),
                    ..QueryFilter::default()
                },
            ),
            (
                "model",
                QueryFilter {
                    model: Some("gpt-5".to_string()),
                    ..QueryFilter::default()
                },
            ),
            (
                "project",
                QueryFilter {
                    project_hash: Some("project-a".to_string()),
                    ..QueryFilter::default()
                },
            ),
            (
                "host",
                QueryFilter {
                    host_id: Some("local".to_string()),
                    ..QueryFilter::default()
                },
            ),
        ];

        for (shape, filter) in shapes {
            let (sql, plan) = projection_plan(
                &dashboard,
                &TopSessionsQuery {
                    filter,
                    ..TopSessionsQuery::default()
                },
            )?;
            assert!(
                sql.contains(&format!("INDEXED BY {TOP_SESSIONS_COVER_INDEX}")),
                "unbounded {shape} projection must force the accepted index: {sql}"
            );
            assert!(
                plan.contains(&format!("USING COVERING INDEX {TOP_SESSIONS_COVER_INDEX}")),
                "unbounded {shape} projection must be covering: {plan}"
            );
        }
        Ok(())
    }

    #[test]
    fn bounded_projection_shapes_keep_event_at_planner_freedom() -> Result<()> {
        let (_temp, store) = fixture()?;
        let dashboard = Dashboard::open(&store)?;
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let shapes = [
            (
                "since",
                QueryFilter {
                    since: Some(date),
                    ..QueryFilter::default()
                },
            ),
            (
                "until",
                QueryFilter {
                    until: Some(date),
                    ..QueryFilter::default()
                },
            ),
            (
                "range",
                QueryFilter {
                    since: Some(date),
                    until: Some(date),
                    ..QueryFilter::default()
                },
            ),
        ];

        for (shape, filter) in shapes {
            let (sql, plan) = projection_plan(
                &dashboard,
                &TopSessionsQuery {
                    filter,
                    ..TopSessionsQuery::default()
                },
            )?;
            assert!(
                !sql.contains("INDEXED BY"),
                "bounded {shape} projection must not force an index: {sql}"
            );
            assert!(
                plan.contains("USING INDEX idx_usage_event_event_at"),
                "bounded {shape} projection should retain the date-range index plan: {plan}"
            );
            assert!(
                !plan.contains(TOP_SESSIONS_COVER_INDEX),
                "bounded {shape} projection should not scan the unbounded covering index: {plan}"
            );
        }
        Ok(())
    }

    #[test]
    fn v24_index_expression_and_projection_identity_are_exactly_in_sync() -> Result<()> {
        let (_temp, store) = fixture()?;
        let conn = store.open_connection()?;
        let actual: String = conn.query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'index' AND name = ?1",
            [TOP_SESSIONS_COVER_INDEX],
            |row| row.get(0),
        )?;
        let expected = exact_top_sessions_cover_index_sql();
        assert_eq!(compact_sql(&actual), compact_sql(&expected));
        Ok(())
    }

    #[test]
    fn bounded_top_k_matches_full_sort_for_pseudorandom_ties() {
        for seed in 0_u64..32 {
            let mut state = seed.wrapping_add(1);
            let rows = (0..200)
                .map(|index| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let primary = ((state >> 32) % 11) as i64;
                    ranking_row(index, primary)
                })
                .collect::<Vec<_>>();
            for sort in [
                TopSessionsSort::Tokens,
                TopSessionsSort::Duration,
                TopSessionsSort::Cost,
            ] {
                for limit in [1, 3, 10, 50] {
                    let mut oracle = rows.clone();
                    oracle.sort_by(|a, b| compare_rows(a, b, sort));
                    oracle.truncate(limit);
                    let bounded_ids = if sort == TopSessionsSort::Duration {
                        select_top_rows(rows.clone(), limit, sort)
                            .into_iter()
                            .map(|row| row.session_id)
                            .collect::<Vec<_>>()
                    } else {
                        select_top_accumulators(
                            rows.iter()
                                .map(|row| (row.session_id.clone(), ranking_accumulator(row)))
                                .collect(),
                            limit,
                            sort,
                        )
                        .into_iter()
                        .map(|(session_id, _)| session_id)
                        .collect::<Vec<_>>()
                    };
                    assert_eq!(
                        bounded_ids,
                        oracle
                            .into_iter()
                            .map(|row| row.session_id)
                            .collect::<Vec<_>>(),
                        "seed={seed}, sort={sort:?}, limit={limit}"
                    );
                }
            }
        }
    }

    #[test]
    fn active_time_parses_each_event_once_and_keeps_legacy_semantics() {
        let times = [
            "2026-05-01T00:00:00Z".to_string(),
            "2026-05-01T00:30:00Z".to_string(),
            "2026-05-01T01:00:00+00:00".to_string(),
            "invalid".to_string(),
        ];
        let mut parse_count = 0;
        let candidate = session_time_span_with(&times, |raw| {
            parse_count += 1;
            DateTime::parse_from_rfc3339(raw).ok()
        });
        assert_eq!(parse_count, times.len());
        assert_eq!(
            candidate,
            session_time_span_legacy(&times, &times[0], &times[times.len() - 1])
        );
    }

    fn ranking_row(index: usize, primary: i64) -> TopSessionRow {
        TopSessionRow {
            session_id: format!("session-{index:03}"),
            session_label: None,
            project_label: None,
            source: Some("codex".to_string()),
            first_event_at: "2026-05-01T00:00:00Z".to_string(),
            last_event_at: "2026-05-01T00:00:00Z".to_string(),
            total_tokens: primary,
            output_tokens: 0,
            cost_usd: primary as f64,
            span_minutes: 0,
            active_minutes: primary,
            event_count: 1,
        }
    }

    fn ranking_accumulator(row: &TopSessionRow) -> SessionAccumulator {
        let mut cost_usd = SqliteFloatSum::default();
        cost_usd.add(row.cost_usd);
        SessionAccumulator {
            session_label: None,
            project_label: None,
            source: row.source.clone(),
            total_tokens: row.total_tokens,
            output_tokens: 0,
            cost_usd,
            event_times: vec![row.first_event_at.clone()],
            event_count: 1,
        }
    }

    fn load_legacy(dashboard: &Dashboard, query: &TopSessionsQuery) -> Result<Vec<TopSessionRow>> {
        let limit = normalize_limit(query.limit);
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
                MAX(e.event_at) AS last_at
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
            Some(all_session_event_times_legacy(dashboard, &query.filter)?)
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
                owned_times =
                    session_event_times_legacy(dashboard, &query.filter, &row.session_id)?;
                &owned_times
            };
            let (span, active) =
                session_time_span_legacy(times, &row.first_event_at, &row.last_event_at);
            row.span_minutes = span;
            row.active_minutes = active;
            rows.push(row);
        }
        rows.sort_by(|a, b| compare_rows(a, b, query.sort));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    fn all_session_event_times_legacy(
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

    fn session_event_times_legacy(
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

    fn session_time_span_legacy(times: &[String], first_at: &str, last_at: &str) -> (i64, i64) {
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
}
