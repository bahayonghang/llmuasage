use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[cfg(test)]
use rusqlite::OpenFlags;
use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};

use super::{Dashboard, DiagnosticsPayload, HomeOverviewSnapshot, QueryFilter};
use crate::{
    domain::source_descriptor::registered_source_descriptors, error::Result, util::now_utc,
};
#[cfg(test)]
use crate::{paths::AppPaths, store::Store};

const HOME_SESSION_IDENTITY: &str =
    "COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key)";

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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

pub(super) fn load_compact(
    dashboard: &Dashboard,
    filter: &QueryFilter,
) -> Result<HomeOverviewSnapshot> {
    let aggregates = load_home_aggregates(&dashboard.conn, filter, false)?;
    Ok(HomeOverviewSnapshot {
        summary: aggregates.summary,
        by_platform: aggregates.by_platform,
    })
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
    super::timezone::register_functions(&conn)?;
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
    let aggregates = load_home_aggregates(&dashboard.conn, filter, true)?;
    let event_read_elapsed = event_read_started.map(|started| started.elapsed());
    if let (Some(elapsed), Some(timing)) = (event_read_elapsed, timing.as_mut()) {
        timing.event_read = elapsed;
        timing.summary = elapsed;
        timing.by_platform = Duration::ZERO;
        timing.series = Duration::ZERO;
    }

    let summary = aggregates.summary;
    let by_platform = aggregates.by_platform;
    let series = aggregates.series;

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

struct HomeAggregates {
    summary: HomeOverviewSummary,
    by_platform: BTreeMap<String, HomeOverviewPlatformStats>,
    series: Vec<HomeOverviewSeriesItem>,
}

fn load_home_aggregates(
    conn: &Connection,
    filter: &QueryFilter,
    include_series: bool,
) -> Result<HomeAggregates> {
    let sql_filter = filter.event_filter(None);
    let local_date = filter.local_date_expr("event_at");
    let summary_sql = format!(
        r#"
        /* home_overview_summary */
        SELECT
            COUNT(*) AS total_requests,
            COUNT(DISTINCT source || ':' || {HOME_SESSION_IDENTITY}) AS total_sessions,
            COALESCE(SUM(total_tokens), 0) AS total_tokens,
            COALESCE(SUM(cost_with_cache_usd), 0.0) AS total_cost_usd,
            COALESCE(SUM(input_tokens), 0) AS input_tokens,
            COALESCE(SUM(cache_creation_tokens), 0) AS cache_creation_tokens,
            COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens,
            COUNT(DISTINCT {local_date}) AS active_days,
            COUNT(DISTINCT source) AS platforms
        FROM usage_event
        {}
        "#,
        sql_filter.where_sql()
    );
    let mut summary_stmt = conn.prepare(&summary_sql)?;
    let (mut summary, input_tokens, cache_creation_tokens, cache_read_tokens) = summary_stmt
        .query_row(params_from_iter(sql_filter.params().iter()), |row| {
            Ok((
                HomeOverviewSummary {
                    total_requests: row.get(0)?,
                    total_sessions: row.get(1)?,
                    total_tokens: row.get(2)?,
                    total_cost_usd: row.get(3)?,
                    cache_efficiency: 0.0,
                    active_days: row.get(7)?,
                    platforms: row.get(8)?,
                },
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;
    let cache_denominator = input_tokens + cache_creation_tokens + cache_read_tokens;
    if cache_denominator != 0 {
        summary.cache_efficiency = cache_read_tokens as f64 / cache_denominator as f64;
    }

    let platform_sql = format!(
        r#"
        /* home_overview_by_platform */
        SELECT
            source,
            COUNT(DISTINCT {HOME_SESSION_IDENTITY}) AS sessions,
            COUNT(*) AS requests,
            COALESCE(SUM(total_tokens), 0) AS tokens
        FROM usage_event
        {}
        GROUP BY source
        "#,
        sql_filter.where_sql()
    );
    let mut platform_stmt = conn.prepare(&platform_sql)?;
    let platform_rows =
        platform_stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                HomeOverviewPlatformStats {
                    sessions: row.get(1)?,
                    requests: row.get(2)?,
                    tokens: row.get(3)?,
                },
            ))
        })?;
    let mut by_platform = default_platform_map();
    for row in platform_rows {
        let (source, stats) = row?;
        by_platform.insert(source, stats);
    }

    let series = if include_series {
        load_home_series(conn, &sql_filter, &local_date)?
    } else {
        Vec::new()
    };

    Ok(HomeAggregates {
        summary,
        by_platform,
        series,
    })
}

fn load_home_series(
    conn: &Connection,
    sql_filter: &super::filter::SqlFilter,
    local_date: &str,
) -> Result<Vec<HomeOverviewSeriesItem>> {
    let series_sql = format!(
        r#"
        /* home_overview_series */
        SELECT
            {local_date} AS local_date,
            source,
            COUNT(DISTINCT {HOME_SESSION_IDENTITY}) AS sessions,
            COUNT(*) AS requests,
            COALESCE(SUM(total_tokens), 0) AS tokens
        FROM usage_event
        {}
        GROUP BY local_date, source
        ORDER BY local_date ASC
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&series_sql)?;
    let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            HomeOverviewPlatformStats {
                sessions: row.get(2)?,
                requests: row.get(3)?,
                tokens: row.get(4)?,
            },
        ))
    })?;
    let mut by_date: BTreeMap<String, HomeOverviewSeriesItem> = BTreeMap::new();
    for row in rows {
        let (date, source, stats) = row?;
        let item = by_date
            .entry(date.clone())
            .or_insert_with(|| HomeOverviewSeriesItem {
                date,
                ..Default::default()
            });
        match source.as_str() {
            "claude" => item.claude = stats,
            "codex" => item.codex = stats,
            "antigravity" => item.antigravity = stats,
            "opencode" => item.opencode = stats,
            _ => {}
        }
    }
    Ok(by_date.into_values().collect())
}

#[cfg(test)]
fn collect_query_plan_evidence(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<BTreeMap<String, QueryPlanEvidence>> {
    let sql_filter = filter.event_filter(None);
    let local_date = filter.local_date_expr("event_at");
    let sql = format!(
        "SELECT COUNT(*) , COUNT(DISTINCT source || ':' || {HOME_SESSION_IDENTITY}), COUNT(DISTINCT {local_date}) FROM usage_event {}",
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

fn last_completed_usage_run(conn: &Connection) -> Result<Option<String>> {
    // `hook-run` is retained as a historical run_log label for old databases.
    Ok(conn.query_row(
        "SELECT MAX(finished_at) FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success'",
        [],
        |row| row.get(0),
    )?)
}

fn has_successful_usage_run(conn: &Connection) -> Result<bool> {
    // `hook-run` is retained as a historical run_log label for old databases.
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success')",
        [],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?)
}

fn default_platform_map() -> BTreeMap<String, HomeOverviewPlatformStats> {
    registered_source_descriptors()
        .iter()
        .map(|descriptor| {
            (
                descriptor.stable_id.to_string(),
                HomeOverviewPlatformStats::default(),
            )
        })
        .collect()
}
