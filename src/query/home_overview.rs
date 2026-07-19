use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

#[cfg(test)]
use rusqlite::OpenFlags;
use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use super::{Dashboard, DiagnosticsPayload, QueryFilter};
use crate::{error::Result, util::now_utc};
#[cfg(test)]
use crate::{paths::AppPaths, store::Store};

const HOME_PLATFORMS: [&str; 4] = ["claude", "codex", "antigravity", "opencode"];

/// Homepage-oriented usage payload consumed by ccr-ui's home overview adapter.
#[derive(Debug, Clone, Serialize)]
pub struct HomeOverviewPayload {
    /// Cross-platform request/session/token summary.
    pub summary: HomeOverviewSummary,
    /// Per-platform totals keyed by source id.
    pub by_platform: BTreeMap<String, HomeOverviewPlatformStats>,
    /// Daily per-platform series in the selected filter timezone.
    pub series: Vec<HomeOverviewSeriesItem>,
    /// Import/index bootstrap hints; session-index fields are ccr-ui-owned in M0.
    pub bootstrap: HomeOverviewBootstrap,
    /// Archive/source diagnostics. Backed by [`Dashboard::diagnostics`] since
    /// 0.5.0-rc.3; before M2 the `by_source` field returned an empty vec.
    pub archive: DiagnosticsPayload,
    /// Latest completed local usage/import activity, or generation time when absent.
    pub last_updated: String,
}

/// Compact totals for the ccr-ui home overview cards.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HomeOverviewSummary {
    pub total_sessions: i64,
    pub total_requests: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub cache_efficiency: f64,
    pub active_days: i64,
    pub platforms: i64,
}

/// Per-platform home overview totals.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HomeOverviewPlatformStats {
    pub sessions: i64,
    pub requests: i64,
    pub tokens: i64,
}

/// One daily home overview trend row with stable platform keys.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HomeOverviewSeriesItem {
    pub date: String,
    pub claude: HomeOverviewPlatformStats,
    pub codex: HomeOverviewPlatformStats,
    pub antigravity: HomeOverviewPlatformStats,
    pub opencode: HomeOverviewPlatformStats,
}

/// Bootstrap hints for first-run ccr-ui screens.
#[derive(Debug, Clone, Serialize)]
pub struct HomeOverviewBootstrap {
    pub usage_import_attempted: bool,
    pub usage_imported_records: i64,
    pub session_reindex_attempted: bool,
    pub indexed_sessions: i64,
    pub usage_job_id: Option<String>,
    pub session_job_id: Option<String>,
    pub needs_usage_import: bool,
    pub needs_session_index: bool,
    pub is_warm: bool,
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub(super) struct HomeOverviewTiming {
    pub total: Duration,
    pub event_read: Duration,
    pub summary: Duration,
    pub by_platform: Duration,
    pub series: Duration,
    pub run_state: Duration,
    pub diagnostics: Duration,
    pub plans: BTreeMap<String, QueryPlanEvidence>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub(super) struct QueryPlanEvidence {
    pub details: Vec<String>,
    pub opcode_count: usize,
}

pub(super) fn load(dashboard: &Dashboard, filter: &QueryFilter) -> Result<HomeOverviewPayload> {
    load_inner(dashboard, filter, None).map(|(payload, _)| payload)
}

#[cfg(test)]
pub(super) fn load_profile(
    dashboard: &Dashboard,
    filter: &QueryFilter,
) -> Result<(HomeOverviewPayload, HomeOverviewTiming)> {
    load_inner(dashboard, filter, Some(HomeOverviewTiming::default()))
        .map(|(payload, timing)| (payload, timing.expect("profile timing")))
}

#[cfg(test)]
pub(super) fn load_profile_read_only(
    db_path: &std::path::Path,
    filter: &QueryFilter,
) -> Result<(HomeOverviewPayload, HomeOverviewTiming)> {
    let root = db_path
        .parent()
        .expect("database path must have a parent")
        .to_path_buf();
    let paths = AppPaths::with_root(root)?;
    let store = Store::new(&paths)?;
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let dashboard = Dashboard { store, conn };
    load_profile(&dashboard, filter)
}

fn load_inner(
    dashboard: &Dashboard,
    filter: &QueryFilter,
    mut timing: Option<HomeOverviewTiming>,
) -> Result<(HomeOverviewPayload, Option<HomeOverviewTiming>)> {
    let total_started = timing.as_ref().map(|_| Instant::now());
    let generated_at = now_utc();

    let event_read_started = timing.as_ref().map(|_| Instant::now());
    let events = load_event_rows(&dashboard.conn, filter)?;
    let event_read_elapsed = event_read_started.map(|started| started.elapsed());
    if let (Some(elapsed), Some(timing)) = (event_read_elapsed, timing.as_mut()) {
        timing.event_read = elapsed;
    }

    let summary_started = timing.as_ref().map(|_| Instant::now());
    let summary = summarize_events(&events);
    if let (Some(started), Some(timing)) = (summary_started, timing.as_mut()) {
        timing.summary = event_read_elapsed.unwrap_or_default() + started.elapsed();
    }

    let by_platform_started = timing.as_ref().map(|_| Instant::now());
    let by_platform = summarize_by_platform(&events);
    if let (Some(started), Some(timing)) = (by_platform_started, timing.as_mut()) {
        timing.by_platform = started.elapsed();
    }

    let series_started = timing.as_ref().map(|_| Instant::now());
    let series = summarize_series(&events);
    if let (Some(started), Some(timing)) = (series_started, timing.as_mut()) {
        timing.series = started.elapsed();
    }

    let run_state_started = timing.as_ref().map(|_| Instant::now());
    let last_updated =
        last_completed_usage_run(&dashboard.conn)?.unwrap_or_else(|| generated_at.clone());
    let has_success = has_successful_usage_run(&dashboard.conn)?;
    if let (Some(started), Some(timing)) = (run_state_started, timing.as_mut()) {
        timing.run_state = started.elapsed();
    }
    let bootstrap = HomeOverviewBootstrap {
        usage_import_attempted: has_success || summary.total_requests > 0,
        usage_imported_records: summary.total_requests,
        session_reindex_attempted: false,
        indexed_sessions: 0,
        usage_job_id: None,
        session_job_id: None,
        needs_usage_import: summary.total_requests == 0,
        needs_session_index: false,
        is_warm: has_success,
    };

    let diagnostics_started = timing.as_ref().map(|_| Instant::now());
    let archive = dashboard.diagnostics()?;
    if let (Some(started), Some(timing)) = (diagnostics_started, timing.as_mut()) {
        timing.diagnostics = started.elapsed();
    }

    #[cfg(test)]
    if let Some(timing) = timing.as_mut() {
        timing.plans = collect_query_plan_evidence(&dashboard.conn, filter)?;
        timing.total = total_started.expect("profile timing start").elapsed();
    }

    let payload = HomeOverviewPayload {
        summary,
        by_platform,
        series,
        bootstrap,
        archive,
        last_updated,
    };
    #[cfg(not(test))]
    let _ = total_started;
    Ok((payload, timing))
}

#[cfg(test)]
fn collect_query_plan_evidence(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<BTreeMap<String, QueryPlanEvidence>> {
    let sql_filter = filter.event_filter(None);
    let modifier = filter.local_time_modifier();
    let sql = format!(
        "SELECT source, event_key, session_id, source_path_hash, date(event_at, '{modifier}'), input_tokens, cache_creation_tokens, cache_read_tokens, total_tokens, cost_with_cache_usd FROM usage_event {}",
        sql_filter.where_sql()
    );
    let mut plan_stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
    let details = plan_stmt
        .query_map(params_from_iter(sql_filter.params().iter()), |row| {
            row.get::<_, String>(3)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut opcode_stmt = conn.prepare(&format!("EXPLAIN {sql}"))?;
    let opcode_count = opcode_stmt
        .query_map(params_from_iter(sql_filter.params().iter()), |_row| Ok(()))?
        .count();
    let shared = QueryPlanEvidence {
        details,
        opcode_count,
    };
    let mut evidence = BTreeMap::new();
    for name in ["event_read", "summary", "by_platform", "series"] {
        evidence.insert(name.to_string(), shared.clone());
    }
    Ok(evidence)
}

#[derive(Debug)]
struct HomeOverviewEvent {
    source: String,
    event_key: String,
    session_id: Option<String>,
    source_path_hash: Option<String>,
    local_date: String,
    input_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    total_tokens: i64,
    cost_with_cache_usd: f64,
}

fn load_event_rows(conn: &Connection, filter: &QueryFilter) -> Result<Vec<HomeOverviewEvent>> {
    let sql_filter = filter.event_filter(None);
    let modifier = filter.local_time_modifier();
    let sql = format!(
        r#"
        SELECT
            source,
            event_key,
            session_id,
            source_path_hash,
            date(event_at, '{modifier}') AS local_date,
            input_tokens,
            cache_creation_tokens,
            cache_read_tokens,
            total_tokens,
            cost_with_cache_usd
        FROM usage_event
        {}
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
        Ok(HomeOverviewEvent {
            source: row.get(0)?,
            event_key: row.get(1)?,
            session_id: row.get(2)?,
            source_path_hash: row.get(3)?,
            local_date: row.get(4)?,
            input_tokens: row.get(5)?,
            cache_creation_tokens: row.get(6)?,
            cache_read_tokens: row.get(7)?,
            total_tokens: row.get(8)?,
            cost_with_cache_usd: row.get(9)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Default)]
struct EventAggregate {
    sessions: HashSet<String>,
    requests: i64,
    tokens: i64,
}

fn session_key(event: &HomeOverviewEvent) -> String {
    let identity = event
        .session_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            event
                .source_path_hash
                .as_deref()
                .filter(|value| !value.is_empty())
        })
        .unwrap_or(&event.event_key);
    format!("{}:{identity}", event.source)
}

fn summarize_events(events: &[HomeOverviewEvent]) -> HomeOverviewSummary {
    let mut sessions = HashSet::new();
    let mut active_days = HashSet::new();
    let mut platforms = HashSet::new();
    let mut total_tokens = 0;
    let mut total_cost_usd = 0.0;
    let mut input_tokens = 0;
    let mut cache_creation_tokens = 0;
    let mut cache_read_tokens = 0;
    for event in events {
        sessions.insert(session_key(event));
        active_days.insert(&event.local_date);
        platforms.insert(&event.source);
        total_tokens += event.total_tokens;
        total_cost_usd += event.cost_with_cache_usd;
        input_tokens += event.input_tokens;
        cache_creation_tokens += event.cache_creation_tokens;
        cache_read_tokens += event.cache_read_tokens;
    }
    let cache_denominator = input_tokens + cache_creation_tokens + cache_read_tokens;
    HomeOverviewSummary {
        total_sessions: sessions.len() as i64,
        total_requests: events.len() as i64,
        total_tokens,
        total_cost_usd,
        cache_efficiency: if cache_denominator == 0 {
            0.0
        } else {
            cache_read_tokens as f64 / cache_denominator as f64
        },
        active_days: active_days.len() as i64,
        platforms: platforms.len() as i64,
    }
}

fn summarize_by_platform(
    events: &[HomeOverviewEvent],
) -> BTreeMap<String, HomeOverviewPlatformStats> {
    let mut aggregates: HashMap<String, EventAggregate> = HashMap::new();
    for event in events {
        let aggregate = aggregates.entry(event.source.clone()).or_default();
        aggregate.sessions.insert(session_key(event));
        aggregate.requests += 1;
        aggregate.tokens += event.total_tokens;
    }
    let mut result = default_platform_map();
    for (source, aggregate) in aggregates {
        result.insert(
            source,
            HomeOverviewPlatformStats {
                sessions: aggregate.sessions.len() as i64,
                requests: aggregate.requests,
                tokens: aggregate.tokens,
            },
        );
    }
    result
}

fn summarize_series(events: &[HomeOverviewEvent]) -> Vec<HomeOverviewSeriesItem> {
    let mut aggregates: BTreeMap<String, HashMap<String, EventAggregate>> = BTreeMap::new();
    for event in events {
        let by_source = aggregates.entry(event.local_date.clone()).or_default();
        let aggregate = by_source.entry(event.source.clone()).or_default();
        aggregate.sessions.insert(session_key(event));
        aggregate.requests += 1;
        aggregate.tokens += event.total_tokens;
    }
    aggregates
        .into_iter()
        .map(|(date, by_source)| {
            let mut item = HomeOverviewSeriesItem {
                date,
                ..Default::default()
            };
            for (source, aggregate) in by_source {
                let stats = HomeOverviewPlatformStats {
                    sessions: aggregate.sessions.len() as i64,
                    requests: aggregate.requests,
                    tokens: aggregate.tokens,
                };
                match source.as_str() {
                    "claude" => item.claude = stats,
                    "codex" => item.codex = stats,
                    "antigravity" => item.antigravity = stats,
                    "opencode" => item.opencode = stats,
                    _ => {}
                }
            }
            item
        })
        .collect()
}

fn last_completed_usage_run(conn: &Connection) -> Result<Option<String>> {
    Ok(conn.query_row(
        "SELECT MAX(finished_at) FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success'",
        [],
        |row| row.get(0),
    )?)
}

fn has_successful_usage_run(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success')",
        [],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?)
}

fn default_platform_map() -> BTreeMap<String, HomeOverviewPlatformStats> {
    HOME_PLATFORMS
        .into_iter()
        .map(|platform| (platform.to_string(), HomeOverviewPlatformStats::default()))
        .collect()
}
