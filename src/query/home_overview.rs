use std::collections::BTreeMap;

use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use super::{Dashboard, DiagnosticsPayload, QueryFilter};
use crate::{error::Result, util::now_utc};

const HOME_PLATFORMS: [&str; 4] = ["claude", "codex", "gemini", "opencode"];

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
    pub gemini: HomeOverviewPlatformStats,
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

pub(super) fn load(dashboard: &Dashboard, filter: &QueryFilter) -> Result<HomeOverviewPayload> {
    let generated_at = now_utc();
    let summary = load_summary(&dashboard.conn, filter)?;
    let by_platform = load_by_platform(&dashboard.conn, filter)?;
    let series = load_series(&dashboard.conn, filter)?;
    let last_updated =
        last_completed_usage_run(&dashboard.conn)?.unwrap_or_else(|| generated_at.clone());
    let has_success = has_successful_usage_run(&dashboard.conn)?;
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
    let archive = dashboard.diagnostics()?;

    Ok(HomeOverviewPayload {
        summary,
        by_platform,
        series,
        bootstrap,
        archive,
        last_updated,
    })
}

fn load_summary(conn: &Connection, filter: &QueryFilter) -> Result<HomeOverviewSummary> {
    let sql_filter = filter.event_filter(None);
    let modifier = filter.local_time_modifier();
    let sql = format!(
        r#"
        SELECT
            COUNT(DISTINCT source || ':' || COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key)),
            COUNT(*),
            COALESCE(SUM(input_tokens), 0) +
                COALESCE(SUM(cache_creation_tokens), 0) +
                COALESCE(SUM(cache_read_tokens), 0) +
                COALESCE(SUM(output_tokens), 0) +
                COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(cost_with_cache_usd), 0.0),
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cache_creation_tokens), 0),
            COALESCE(SUM(cache_read_tokens), 0),
            COUNT(DISTINCT date(event_at, '{modifier}')),
            COUNT(DISTINCT source)
        FROM usage_event
        {}
        "#,
        sql_filter.where_sql()
    );

    Ok(
        conn.query_row(&sql, params_from_iter(sql_filter.params().iter()), |row| {
            let input_tokens = row.get::<_, Option<i64>>(4)?.unwrap_or_default();
            let cache_creation_tokens = row.get::<_, Option<i64>>(5)?.unwrap_or_default();
            let cache_read_tokens = row.get::<_, Option<i64>>(6)?.unwrap_or_default();
            let cache_denominator = input_tokens + cache_creation_tokens + cache_read_tokens;
            Ok(HomeOverviewSummary {
                total_sessions: row.get::<_, Option<i64>>(0)?.unwrap_or_default(),
                total_requests: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                total_cost_usd: row.get::<_, Option<f64>>(3)?.unwrap_or_default(),
                cache_efficiency: if cache_denominator == 0 {
                    0.0
                } else {
                    cache_read_tokens as f64 / cache_denominator as f64
                },
                active_days: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                platforms: row.get::<_, Option<i64>>(8)?.unwrap_or_default(),
            })
        })?,
    )
}

fn load_by_platform(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<BTreeMap<String, HomeOverviewPlatformStats>> {
    let sql_filter = filter.event_filter(None);
    let sql = format!(
        r#"
        SELECT
            source,
            COUNT(DISTINCT source || ':' || COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key)),
            COUNT(*),
            COALESCE(SUM(input_tokens), 0) +
                COALESCE(SUM(cache_creation_tokens), 0) +
                COALESCE(SUM(cache_read_tokens), 0) +
                COALESCE(SUM(output_tokens), 0) +
                COALESCE(SUM(reasoning_output_tokens), 0)
        FROM usage_event
        {}
        GROUP BY source
        ORDER BY source ASC
        "#,
        sql_filter.where_sql()
    );
    let mut rows = conn.prepare(&sql)?;
    let mut result = default_platform_map();
    let mapped = rows.query_map(params_from_iter(sql_filter.params().iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            HomeOverviewPlatformStats {
                sessions: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                requests: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
            },
        ))
    })?;

    for item in mapped {
        let (source, stats) = item?;
        result.insert(source, stats);
    }
    Ok(result)
}

fn load_series(conn: &Connection, filter: &QueryFilter) -> Result<Vec<HomeOverviewSeriesItem>> {
    let sql_filter = filter.event_filter(None);
    let modifier = filter.local_time_modifier();
    let sql = format!(
        r#"
        SELECT
            date(event_at, '{modifier}') AS local_date,
            source,
            COUNT(DISTINCT source || ':' || COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key)),
            COUNT(*),
            COALESCE(SUM(input_tokens), 0) +
                COALESCE(SUM(cache_creation_tokens), 0) +
                COALESCE(SUM(cache_read_tokens), 0) +
                COALESCE(SUM(output_tokens), 0) +
                COALESCE(SUM(reasoning_output_tokens), 0)
        FROM usage_event
        {}
        GROUP BY local_date, source
        ORDER BY local_date ASC, source ASC
        "#,
        sql_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            HomeOverviewPlatformStats {
                sessions: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                requests: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
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
            "gemini" => item.gemini = stats,
            "opencode" => item.opencode = stats,
            _ => {}
        }
    }
    Ok(by_date.into_values().collect())
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
