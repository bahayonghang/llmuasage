use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{
    store::{IntegrationState, RunRecord, Store},
    util::now_utc,
};

#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenSummary {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewPayload {
    pub generated_at: String,
    pub total: TokenSummary,
    pub last_24h: TokenSummary,
    pub source_count: i64,
    pub bucket_count: i64,
    pub last_sync_at: Option<String>,
    pub last_export_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    pub label: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelBreakdown {
    pub model: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceBreakdown {
    pub source: String,
    pub total_tokens: i64,
    pub last_event_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectBreakdown {
    pub project_hash: String,
    pub project_label: String,
    pub project_ref: Option<String>,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLine {
    pub source: String,
    pub model: String,
    pub total_tokens: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CursorHealth {
    pub source: String,
    pub cursor_key: String,
    pub updated_at: Option<String>,
    pub sqlite_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthPayload {
    pub integrations: Vec<IntegrationState>,
    pub cursors: Vec<CursorHealth>,
    pub recent_failures: Vec<RunRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    pub overview: OverviewPayload,
    pub day_trends: Vec<TrendPoint>,
    pub week_trends: Vec<TrendPoint>,
    pub month_trends: Vec<TrendPoint>,
    pub all_trends: Vec<TrendPoint>,
    pub models: Vec<ModelBreakdown>,
    pub sources: Vec<SourceBreakdown>,
    pub projects: Vec<ProjectBreakdown>,
    pub costs: Vec<CostLine>,
    pub health: HealthPayload,
}

pub fn load_overview(store: &Store) -> Result<OverviewPayload> {
    let conn = store.open_connection()?;
    let total = query_token_summary(&conn, None)?;
    let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339();
    let last_24h = query_token_summary(&conn, Some(&cutoff))?;
    let source_count = scalar_i64(
        &conn,
        "SELECT COUNT(DISTINCT source) FROM usage_bucket_30m",
        [],
    )?;
    let bucket_count = scalar_i64(&conn, "SELECT COUNT(*) FROM usage_bucket_30m", [])?;
    let last_sync_at = scalar_optional_string(
        &conn,
        "SELECT MAX(finished_at) FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success'",
        [],
    )?;
    let last_export_at = scalar_optional_string(
        &conn,
        "SELECT MAX(finished_at) FROM run_log WHERE command = 'export html' AND status = 'success'",
        [],
    )?;

    Ok(OverviewPayload {
        generated_at: now_utc(),
        total,
        last_24h,
        source_count,
        bucket_count,
        last_sync_at,
        last_export_at,
    })
}

pub fn load_trends(store: &Store, window: &str) -> Result<Vec<TrendPoint>> {
    let conn = store.open_connection()?;
    let (sql, cutoff): (&str, Option<String>) = match window {
        "day" => (
            "SELECT hour_start AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m WHERE hour_start >= ?1 GROUP BY hour_start ORDER BY hour_start ASC",
            Some((Utc::now() - Duration::hours(24)).to_rfc3339()),
        ),
        "week" => (
            "SELECT substr(hour_start, 1, 10) AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m WHERE hour_start >= ?1 GROUP BY substr(hour_start, 1, 10) ORDER BY label ASC",
            Some((Utc::now() - Duration::days(7)).to_rfc3339()),
        ),
        "month" => (
            "SELECT substr(hour_start, 1, 10) AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m WHERE hour_start >= ?1 GROUP BY substr(hour_start, 1, 10) ORDER BY label ASC",
            Some((Utc::now() - Duration::days(30)).to_rfc3339()),
        ),
        _ => (
            "SELECT substr(hour_start, 1, 7) AS label, SUM(total_tokens) AS total_tokens FROM usage_bucket_30m GROUP BY substr(hour_start, 1, 7) ORDER BY label ASC",
            None,
        ),
    };

    let mut stmt = conn.prepare(sql)?;
    if let Some(cutoff) = cutoff {
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok(TrendPoint {
                label: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    } else {
        let rows = stmt.query_map([], |row| {
            Ok(TrendPoint {
                label: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

pub fn load_model_breakdown(store: &Store) -> Result<Vec<ModelBreakdown>> {
    let conn = store.open_connection()?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            model,
            SUM(input_tokens),
            SUM(cached_input_tokens),
            SUM(output_tokens),
            SUM(reasoning_output_tokens),
            SUM(total_tokens)
        FROM usage_bucket_30m
        GROUP BY model
        ORDER BY SUM(total_tokens) DESC, model ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ModelBreakdown {
            model: row.get(0)?,
            input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            cached_input_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
            output_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
            reasoning_output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
            total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn load_source_breakdown(store: &Store) -> Result<Vec<SourceBreakdown>> {
    let conn = store.open_connection()?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            b.source,
            SUM(b.total_tokens),
            MAX(e.event_at)
        FROM usage_bucket_30m b
        LEFT JOIN usage_event e ON e.source = b.source
        GROUP BY b.source
        ORDER BY SUM(b.total_tokens) DESC, b.source ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SourceBreakdown {
            source: row.get(0)?,
            total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            last_event_at: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn load_project_breakdown(store: &Store) -> Result<Vec<ProjectBreakdown>> {
    let conn = store.open_connection()?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            project_hash,
            MAX(project_label),
            MAX(project_ref),
            SUM(total_tokens)
        FROM usage_bucket_30m
        WHERE project_hash <> ''
        GROUP BY project_hash
        ORDER BY SUM(total_tokens) DESC, MAX(project_label) ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ProjectBreakdown {
            project_hash: row.get(0)?,
            project_label: row
                .get::<_, Option<String>>(1)?
                .unwrap_or_else(|| "unknown-project".to_string()),
            project_ref: row.get(2)?,
            total_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn load_cost_breakdown(store: &Store) -> Result<Vec<CostLine>> {
    let conn = store.open_connection()?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            source,
            model,
            SUM(input_tokens),
            SUM(cached_input_tokens),
            SUM(output_tokens),
            SUM(reasoning_output_tokens),
            SUM(total_tokens)
        FROM usage_bucket_30m
        GROUP BY source, model
        ORDER BY SUM(total_tokens) DESC, source ASC, model ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        let source: String = row.get(0)?;
        let model: String = row.get(1)?;
        let input_tokens = row.get::<_, Option<i64>>(2)?.unwrap_or_default();
        let cached_input_tokens = row.get::<_, Option<i64>>(3)?.unwrap_or_default();
        let output_tokens = row.get::<_, Option<i64>>(4)?.unwrap_or_default();
        let reasoning_output_tokens = row.get::<_, Option<i64>>(5)?.unwrap_or_default();
        let total_tokens = row.get::<_, Option<i64>>(6)?.unwrap_or_default();
        let estimated_cost_usd = estimate_cost_usd(
            &source,
            &model,
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
        );

        Ok(CostLine {
            source,
            model,
            total_tokens,
            estimated_cost_usd,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn load_health(store: &Store) -> Result<HealthPayload> {
    let integrations = store.load_integration_states()?;
    let recent_failures = store
        .recent_runs(10)?
        .into_iter()
        .filter(|run| run.status != "success" && run.status != "running")
        .collect::<Vec<_>>();

    let conn = store.open_connection()?;
    let mut stmt = conn.prepare(
        r#"
        SELECT source, cursor_key, updated_at, sqlite_status
        FROM source_cursor
        ORDER BY source ASC, cursor_key ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(CursorHealth {
            source: row.get(0)?,
            cursor_key: row.get(1)?,
            updated_at: row.get(2)?,
            sqlite_status: row.get(3)?,
        })
    })?;

    Ok(HealthPayload {
        integrations,
        cursors: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        recent_failures,
    })
}

pub fn build_dashboard_snapshot(store: &Store) -> Result<DashboardSnapshot> {
    Ok(DashboardSnapshot {
        overview: load_overview(store)?,
        day_trends: load_trends(store, "day")?,
        week_trends: load_trends(store, "week")?,
        month_trends: load_trends(store, "month")?,
        all_trends: load_trends(store, "all")?,
        models: load_model_breakdown(store)?,
        sources: load_source_breakdown(store)?,
        projects: load_project_breakdown(store)?,
        costs: load_cost_breakdown(store)?,
        health: load_health(store)?,
    })
}

fn query_token_summary(conn: &Connection, cutoff: Option<&str>) -> Result<TokenSummary> {
    let sql = if cutoff.is_some() {
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0)
        FROM usage_bucket_30m
        WHERE hour_start >= ?1
        "#
    } else {
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0)
        FROM usage_bucket_30m
        "#
    };

    let mut stmt = conn.prepare(sql)?;
    let summary = if let Some(cutoff) = cutoff {
        stmt.query_row(params![cutoff], map_token_summary)?
    } else {
        stmt.query_row([], map_token_summary)?
    };
    Ok(summary)
}

fn map_token_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenSummary> {
    Ok(TokenSummary {
        input_tokens: row.get(0)?,
        cached_input_tokens: row.get(1)?,
        output_tokens: row.get(2)?,
        reasoning_output_tokens: row.get(3)?,
        total_tokens: row.get(4)?,
    })
}

fn scalar_i64<P>(conn: &Connection, sql: &str, params: P) -> Result<i64>
where
    P: rusqlite::Params,
{
    Ok(conn
        .query_row(sql, params, |row| row.get::<_, Option<i64>>(0))?
        .unwrap_or_default())
}

fn scalar_optional_string<P>(conn: &Connection, sql: &str, params: P) -> Result<Option<String>>
where
    P: rusqlite::Params,
{
    Ok(conn
        .query_row(sql, params, |row| row.get(0))
        .unwrap_or(None))
}

fn estimate_cost_usd(
    source: &str,
    model: &str,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_output_tokens: i64,
) -> f64 {
    let pricing = PRICE_CATALOG
        .iter()
        .find(|entry| {
            entry.source.eq_ignore_ascii_case(source)
                && entry
                    .matchers
                    .iter()
                    .any(|matcher| model.to_ascii_lowercase().contains(matcher))
        })
        .or_else(|| PRICE_CATALOG.iter().find(|entry| entry.source == "*"));

    let Some(pricing) = pricing else {
        return 0.0;
    };

    let input_mtok = input_tokens as f64 / 1_000_000.0;
    let cached_mtok = cached_input_tokens as f64 / 1_000_000.0;
    let output_mtok = (output_tokens + reasoning_output_tokens) as f64 / 1_000_000.0;
    input_mtok * pricing.input_per_mtok
        + cached_mtok * pricing.cached_per_mtok
        + output_mtok * pricing.output_per_mtok
}

struct PriceEntry {
    source: &'static str,
    matchers: &'static [&'static str],
    input_per_mtok: f64,
    cached_per_mtok: f64,
    output_per_mtok: f64,
}

const PRICE_CATALOG: &[PriceEntry] = &[
    PriceEntry {
        source: "codex",
        matchers: &["gpt-5-mini"],
        input_per_mtok: 0.25,
        cached_per_mtok: 0.025,
        output_per_mtok: 2.0,
    },
    PriceEntry {
        source: "codex",
        matchers: &["gpt-5", "o3", "o4"],
        input_per_mtok: 1.25,
        cached_per_mtok: 0.125,
        output_per_mtok: 10.0,
    },
    PriceEntry {
        source: "claude",
        matchers: &["opus"],
        input_per_mtok: 15.0,
        cached_per_mtok: 1.5,
        output_per_mtok: 75.0,
    },
    PriceEntry {
        source: "claude",
        matchers: &["sonnet", "claude-3-7"],
        input_per_mtok: 3.0,
        cached_per_mtok: 0.3,
        output_per_mtok: 15.0,
    },
    PriceEntry {
        source: "opencode",
        matchers: &["gpt", "o3", "o4"],
        input_per_mtok: 1.25,
        cached_per_mtok: 0.125,
        output_per_mtok: 10.0,
    },
    PriceEntry {
        source: "*",
        matchers: &[""],
        input_per_mtok: 0.0,
        cached_per_mtok: 0.0,
        output_per_mtok: 0.0,
    },
];
