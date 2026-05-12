use chrono::{Duration, SecondsFormat, Utc};
use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use crate::{
    error::Result,
    store::{IntegrationState, RunRecord, Store},
    util::now_utc,
};

pub mod filter;
mod heatmap;
mod home_overview;
pub(crate) mod logs;
pub mod pricing;
pub mod pricing_catalog;
pub mod reports;

pub use filter::{QueryFilter, ReportTimezone};
pub use heatmap::HeatmapPoint;
pub use home_overview::{
    HomeOverviewBootstrap, HomeOverviewPayload, HomeOverviewPlatformStats, HomeOverviewSeriesItem,
    HomeOverviewSummary,
};
pub use logs::{LogRecord, LogsPage, LogsQuery};
pub use pricing::{CostBreakdown, PRICING_MIXED, PRICING_UNPRICED, PricingStatus};
pub use pricing_catalog::PricingCatalog;

/// Aggregated token counters returned by overview and trend queries.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenSummary {
    /// Sum of non-cache read tokens.
    pub input_tokens: i64,
    /// Sum of cached/reused input tokens.
    pub cache_read_tokens: i64,
    /// Sum of non-reasoning output tokens.
    pub output_tokens: i64,
    /// Sum of separately reported reasoning tokens.
    pub reasoning_output_tokens: i64,
    /// Total normalized tokens across all categories.
    pub total_tokens: i64,
}

impl TokenSummary {
    /// Combined output tokens ccr-ui should display at API boundaries.
    pub fn output_tokens_with_reasoning(&self) -> i64 {
        self.output_tokens + self.reasoning_output_tokens
    }

    /// Cross-source cache reuse ratio, returning `0.0` when no input was used.
    pub fn cache_efficiency(&self) -> f64 {
        let denominator = self.input_tokens + self.cache_read_tokens;
        if denominator == 0 {
            0.0
        } else {
            self.cache_read_tokens as f64 / denominator as f64
        }
    }
}

/// Top-level dashboard numbers shown in status, web, and export views.
#[derive(Debug, Clone, Serialize)]
pub struct OverviewPayload {
    /// Snapshot generation time in RFC 3339 format.
    pub generated_at: String,
    /// Lifetime totals across the entire dataset.
    pub total: TokenSummary,
    /// Totals restricted to the last 24 hours.
    pub last_24h: TokenSummary,
    /// Distinct source count present in aggregated buckets.
    pub source_count: i64,
    /// Number of persisted 30-minute buckets.
    pub bucket_count: i64,
    /// Lifetime usage event count, summed from `usage_bucket_30m.event_count`.
    pub total_events: i64,
    /// Usage event count restricted to the last 24 hours.
    pub last_24h_events: i64,
    /// Estimated lifetime cost using persisted `cost_with_cache_usd` buckets.
    pub total_cost_usd: f64,
    /// Cross-source cache read ratio for the filtered lifetime total.
    pub cache_efficiency: f64,
    /// Last successful sync/hook-run finish time.
    pub last_sync_at: Option<String>,
    /// Last successful HTML export finish time.
    pub last_export_at: Option<String>,
}

/// One plotted point in a trend series.
#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    /// Display label for the time window bucket.
    pub label: String,
    /// Total tokens in the bucket.
    pub total_tokens: i64,
}

/// One daily trend row produced by [`Dashboard::trends_daily`].
///
/// Output tokens include reasoning tokens (D9): the API surface ccr-ui
/// consumes intentionally collapses output + reasoning into one number.
#[derive(Debug, Clone, Serialize)]
pub struct DailyTrendPoint {
    /// Local calendar date in `YYYY-MM-DD`, computed in [`QueryFilter::timezone`].
    pub date: String,
    /// Summed non-cache prompt tokens.
    pub input_tokens: i64,
    /// Summed cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Summed cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
    /// Output tokens with reasoning tokens already added in (D9).
    pub output_tokens: i64,
    /// Total normalized tokens for the day.
    pub total_tokens: i64,
    /// Number of underlying usage events for the day.
    pub event_count: i64,
    /// Estimated cost for the day using cache-aware pricing.
    pub cost_with_cache_usd: f64,
}

/// Per-model aggregate shown in dashboard breakdowns.
#[derive(Debug, Clone, Serialize)]
pub struct ModelBreakdown {
    /// Normalized model name.
    pub model: String,
    /// Summed non-cache read tokens.
    pub input_tokens: i64,
    /// Summed cache read tokens.
    pub cache_read_tokens: i64,
    /// Summed output tokens.
    pub output_tokens: i64,
    /// Summed reasoning-only output tokens.
    pub reasoning_output_tokens: i64,
    /// Summed total tokens.
    pub total_tokens: i64,
    /// Number of underlying usage events contributing to this row.
    pub event_count: i64,
    /// Estimated cost using cache-aware pricing.
    pub cost_with_cache_usd: f64,
    /// Estimated cost if cache reads were billed as regular input.
    pub cost_without_cache_usd: f64,
    /// Estimated cache savings compared with no-cache pricing.
    pub cache_savings_usd: f64,
    /// Aggregated pricing status for this model (`static`, `snapshot`, `unpriced`, or `mixed`).
    pub pricing_status: String,
    /// Aggregated pricing catalog/source label, or `mixed` when multiple values contributed.
    pub pricing_source: Option<String>,
    /// Aggregated pricing rate JSON, or `mixed` when multiple rates contributed.
    pub pricing_rate: Option<String>,
}

/// Per-source aggregate plus freshest observed event time.
#[derive(Debug, Clone, Serialize)]
pub struct SourceBreakdown {
    /// Source identifier.
    pub source: String,
    /// Summed total tokens for the source.
    pub total_tokens: i64,
    /// Latest raw event timestamp observed for the source.
    pub last_event_at: Option<String>,
    /// Number of underlying usage events for the source.
    pub event_count: i64,
}

/// Per-project aggregate shown in rankings.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectBreakdown {
    /// Stable hashed project key.
    pub project_hash: String,
    /// Human-readable project label.
    pub project_label: String,
    /// Optional repo/project reference.
    pub project_ref: Option<String>,
    /// Summed total tokens for the project.
    pub total_tokens: i64,
    /// Number of underlying usage events for the project.
    pub event_count: i64,
    /// Estimated project cost using cache-aware pricing.
    pub total_cost_usd: f64,
    /// Display-safe project path surrogate. Raw filesystem paths are not
    /// persisted by llmusage; adapters that need a path-like display can use
    /// this stable project reference/label.
    pub project_path: Option<String>,
}

/// Cost estimate line for one `(source, model)` pair.
#[derive(Debug, Clone, Serialize)]
pub struct CostLine {
    /// Source identifier.
    pub source: String,
    /// Normalized model name.
    pub model: String,
    /// Summed total tokens for the pair.
    pub total_tokens: i64,
    /// Estimated USD cost using the persisted cache-aware cost column.
    pub estimated_cost_usd: f64,
    /// Number of underlying usage events for the pair.
    pub event_count: i64,
}

/// Cursor freshness row used in health views.
#[derive(Debug, Clone, Serialize)]
pub struct CursorHealth {
    /// Source identifier.
    pub source: String,
    /// Cursor key within the source.
    pub cursor_key: String,
    /// Last cursor update time, if any.
    pub updated_at: Option<String>,
    /// Source-specific SQLite status field, mainly for OpenCode.
    pub sqlite_status: Option<String>,
}

/// Health payload combining integrations, cursors, and recent failures.
#[derive(Debug, Clone, Serialize)]
pub struct HealthPayload {
    /// Latest install/probe states for known integrations.
    pub integrations: Vec<IntegrationState>,
    /// Cursor freshness/health rows.
    pub cursors: Vec<CursorHealth>,
    /// Recent non-success command runs.
    pub recent_failures: Vec<RunRecord>,
}

/// Per-source archive state diagnostics, derived from the `source_file`
/// state machine (D15 / ADR 0006).
///
/// Field semantics:
/// - `live_files` — files seen by the most recent sync run for this source.
/// - `missing_files` — previously seen, not in the latest run.
/// - `deleted_files` — explicitly forgotten via the diagnostics forget entry.
/// - `recent_completed_at` — last run that finished its `recent_days` window.
///   `None` until `RecentReady` is wired in 4.4.
/// - `history_completed_at` — last run that drove cursors back to the earliest
///   file. `None` until full-history sweeps are tracked.
#[derive(Debug, Clone, Serialize)]
pub struct SourceDiagnostics {
    /// Source identifier (`codex` / `claude` / `opencode` / `gemini`).
    pub source: String,
    /// Number of `source_file` rows currently in `live` state.
    pub live_files: u64,
    /// Number of `source_file` rows currently in `missing` state.
    pub missing_files: u64,
    /// Number of `source_file` rows currently in `deleted_by_user` state.
    pub deleted_files: u64,
    /// Number of tracked source files that are currently absent on disk.
    ///
    /// This is an immediate filesystem check used by the lossy rebuild guard;
    /// it can be non-zero before a normal sync has swept `state='missing'`.
    pub missing_file_count: u64,
    /// Number of imported usage rows that would be protected from a default
    /// lossy `sync --rebuild` because at least one source file is absent.
    pub protected_event_count: u64,
    /// True when the default `sync --rebuild` guard would refuse this source
    /// until files are restored or `--allow-lossy-rebuild` is passed.
    pub lossy_rebuild_risk: bool,
    /// Last RFC 3339 time the recent-window scan reached the cutoff.
    pub recent_completed_at: Option<String>,
    /// Last RFC 3339 time the history scan reached the earliest file.
    pub history_completed_at: Option<String>,
}

/// Top-level diagnostics payload returned by [`Dashboard::diagnostics`] and
/// embedded into `HomeOverviewPayload.archive` (F4.4 / F5.3).
///
/// `archive_root` is `paths.root_dir` as a display string (D28); ccr-ui keeps
/// the legacy field name but the value now points at the llmusage runtime
/// root rather than the old ccr-db archive root.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsPayload {
    /// Absolute path to the llmusage runtime root.
    pub archive_root: String,
    /// One row per source, ordered by source identifier.
    pub by_source: Vec<SourceDiagnostics>,
    /// Most recent failed sync/hook-run records, oldest-first.
    pub recent_failures: Vec<RunRecord>,
}

/// Full snapshot embedded into exported HTML bundles.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    /// Headline overview metrics.
    pub overview: OverviewPayload,
    /// Last 24 hours trend series.
    pub day_trends: Vec<TrendPoint>,
    /// Last 7 days trend series.
    pub week_trends: Vec<TrendPoint>,
    /// Last 30 days trend series.
    pub month_trends: Vec<TrendPoint>,
    /// Lifetime/month-grouped trend series.
    pub all_trends: Vec<TrendPoint>,
    /// Per-model breakdown table.
    pub models: Vec<ModelBreakdown>,
    /// Per-source breakdown table.
    pub sources: Vec<SourceBreakdown>,
    /// Per-project ranking table.
    pub projects: Vec<ProjectBreakdown>,
    /// Per-source/model cost estimate table.
    pub costs: Vec<CostLine>,
    /// Integration/cursor/run health payload.
    pub health: HealthPayload,
}

/// Read-side façade backed by a single SQLite connection. All eight dashboard
/// queries share the same connection so a snapshot only opens the DB once.
pub struct Dashboard {
    pub(super) store: Store,
    pub(super) conn: Connection,
}

impl Dashboard {
    /// Opens a fresh connection bound to `store` and returns a Dashboard ready
    /// to answer any of the dashboard queries.
    pub fn open(store: &Store) -> Result<Self> {
        let conn = store.open_connection()?;
        Ok(Self {
            store: store.clone(),
            conn,
        })
    }

    /// Loads top-level lifetime/24h overview metrics plus recent sync/export timestamps.
    pub fn overview(&self, filter: &QueryFilter) -> Result<OverviewPayload> {
        let total = query_token_summary(&self.conn, filter, None)?;
        let cutoff = (Utc::now() - Duration::hours(24)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let last_24h = query_token_summary(&self.conn, filter, Some(&cutoff))?;
        let total_events = query_event_count(&self.conn, filter, None)?;
        let last_24h_events = query_event_count(&self.conn, filter, Some(&cutoff))?;
        let total_cost_usd = query_cost_with_cache(&self.conn, filter, None)?;
        let cache_efficiency = total.cache_efficiency();
        let bucket_filter = filter.bucket_filter(None);
        let source_count_sql = format!(
            "SELECT COUNT(DISTINCT source) FROM usage_bucket_30m{}",
            bucket_filter.where_sql()
        );
        let source_count = scalar_i64(
            &self.conn,
            &source_count_sql,
            params_from_iter(bucket_filter.params().iter()),
        )?;
        let bucket_count_sql = format!(
            "SELECT COUNT(*) FROM usage_bucket_30m{}",
            bucket_filter.where_sql()
        );
        let bucket_count = scalar_i64(
            &self.conn,
            &bucket_count_sql,
            params_from_iter(bucket_filter.params().iter()),
        )?;
        let last_sync_at = scalar_optional_string(
            &self.conn,
            "SELECT MAX(finished_at) FROM run_log WHERE command IN ('sync', 'hook-run') AND status = 'success'",
            [],
        )?;
        let last_export_at = scalar_optional_string(
            &self.conn,
            "SELECT MAX(finished_at) FROM run_log WHERE command = 'export html' AND status = 'success'",
            [],
        )?;

        Ok(OverviewPayload {
            generated_at: now_utc(),
            total,
            last_24h,
            source_count,
            bucket_count,
            total_events,
            last_24h_events,
            total_cost_usd,
            cache_efficiency,
            last_sync_at,
            last_export_at,
        })
    }

    /// Loads aggregated trend points for the requested window (`day`, `week`, `month`, or `all`).
    ///
    /// Retained for the legacy `/api/trends?window=` HTTP route.
    /// New surfaces should prefer [`Dashboard::trends_daily`] for full token
    /// breakdown and event counts.
    pub fn trends(&self, window: &str, filter: &QueryFilter) -> Result<Vec<TrendPoint>> {
        let mut sql_filter = filter.bucket_filter(None);
        let cutoff = match window {
            "day" => Some(Utc::now() - Duration::hours(24)),
            "week" => Some(Utc::now() - Duration::days(7)),
            "month" => Some(Utc::now() - Duration::days(30)),
            _ => None,
        };
        if let Some(cutoff) = cutoff {
            sql_filter.push(
                "hour_start >= ?",
                cutoff.to_rfc3339_opts(SecondsFormat::Secs, true),
            );
        }
        let modifier = filter.local_time_modifier();
        let label_expr = match window {
            "day" => "hour_start".to_string(),
            "week" | "month" => format!("date(hour_start, '{modifier}')"),
            _ => format!("strftime('%Y-%m', hour_start, '{modifier}')"),
        };
        let sql = format!(
            r#"
            SELECT {label_expr} AS label, SUM(total_tokens) AS total_tokens
            FROM usage_bucket_30m
            {}
            GROUP BY label
            ORDER BY label ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(TrendPoint {
                label: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads a per-day trend series with full token breakdown and event count
    /// (D9/F4.2). Output tokens already include reasoning tokens at the
    /// API boundary; callers must not add them a second time.
    ///
    /// Days are grouped by the local calendar date in
    /// [`QueryFilter::timezone`]; UTC days are reconstructed from the
    /// underlying `hour_start` column when the filter requests UTC.
    pub fn trends_daily(&self, filter: &QueryFilter) -> Result<Vec<DailyTrendPoint>> {
        let sql_filter = filter.bucket_filter(None);
        let modifier = filter.local_time_modifier();
        let sql = format!(
            r#"
            SELECT
                date(hour_start, '{modifier}') AS local_date,
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cache_creation_tokens), 0),
                COALESCE(SUM(output_tokens), 0) + COALESCE(SUM(reasoning_output_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(event_count), 0),
                COALESCE(SUM(cost_with_cache_usd), 0.0)
            FROM usage_bucket_30m
            {}
            GROUP BY local_date
            ORDER BY local_date ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            Ok(DailyTrendPoint {
                date: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cost_with_cache_usd: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads total token usage grouped by normalized model.
    pub fn model_breakdown(&self, filter: &QueryFilter) -> Result<Vec<ModelBreakdown>> {
        let sql_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                model,
                SUM(input_tokens),
                SUM(cache_read_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens),
                SUM(event_count),
                SUM(cost_with_cache_usd),
                SUM(cost_without_cache_usd),
                CASE
                    WHEN COUNT(DISTINCT pricing_status) = 1 THEN MAX(pricing_status)
                    ELSE '{PRICING_MIXED}'
                END,
                CASE
                    WHEN COUNT(DISTINCT COALESCE(pricing_source, '__llmusage_null__')) = 1 THEN MAX(pricing_source)
                    ELSE '{PRICING_MIXED}'
                END,
                CASE
                    WHEN COUNT(DISTINCT COALESCE(pricing_rate, '__llmusage_null__')) = 1 THEN MAX(pricing_rate)
                    ELSE '{PRICING_MIXED}'
                END
            FROM usage_bucket_30m
            {}
            GROUP BY model
            ORDER BY SUM(total_tokens) DESC, model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let cost_with_cache_usd = row.get::<_, Option<f64>>(7)?.unwrap_or_default();
            let cost_without_cache_usd = row.get::<_, Option<f64>>(8)?.unwrap_or_default();
            Ok(ModelBreakdown {
                model: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                reasoning_output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                cost_with_cache_usd,
                cost_without_cache_usd,
                cache_savings_usd: (cost_without_cache_usd - cost_with_cache_usd).max(0.0),
                pricing_status: row
                    .get::<_, Option<String>>(9)?
                    .unwrap_or_else(|| PRICING_UNPRICED.to_string()),
                pricing_source: row.get(10)?,
                pricing_rate: row.get(11)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads total token usage grouped by source plus each source's freshest event time.
    pub fn source_breakdown(&self, filter: &QueryFilter) -> Result<Vec<SourceBreakdown>> {
        let bucket_filter = filter.bucket_filter(None);
        let event_filter = filter.event_filter(None);
        let mut query_params = bucket_filter.clone().into_params();
        query_params.extend(event_filter.clone().into_params());
        let sql = format!(
            r#"
            SELECT
                totals.source,
                totals.total_tokens,
                last_event.last_event_at,
                totals.event_count
            FROM (
                SELECT source, SUM(total_tokens) AS total_tokens, SUM(event_count) AS event_count
                FROM usage_bucket_30m
                {}
                GROUP BY source
            ) totals
            LEFT JOIN (
                SELECT source, MAX(event_at) AS last_event_at
                FROM usage_event
                {}
                GROUP BY source
            ) last_event
                ON last_event.source = totals.source
            ORDER BY totals.total_tokens DESC, totals.source ASC
            "#,
            bucket_filter.where_sql(),
            event_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(query_params.iter()), |row| {
            Ok(SourceBreakdown {
                source: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                last_event_at: row.get(2)?,
                event_count: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads ranked project totals derived from aggregated buckets.
    pub fn project_breakdown(&self, filter: &QueryFilter) -> Result<Vec<ProjectBreakdown>> {
        let mut sql_filter = filter.bucket_filter(None);
        sql_filter.push_raw("project_hash <> ''");
        let sql = format!(
            r#"
            SELECT
                project_hash,
                MAX(project_label),
                MAX(project_ref),
                SUM(total_tokens),
                SUM(event_count),
                SUM(cost_with_cache_usd)
            FROM usage_bucket_30m
            {}
            GROUP BY project_hash
            ORDER BY SUM(total_tokens) DESC, MAX(project_label) ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let project_label = row
                .get::<_, Option<String>>(1)?
                .unwrap_or_else(|| "unknown-project".to_string());
            let project_ref = row.get::<_, Option<String>>(2)?;
            let project_path = project_ref.clone().or_else(|| Some(project_label.clone()));
            Ok(ProjectBreakdown {
                project_hash: row.get(0)?,
                project_label,
                project_ref,
                total_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                total_cost_usd: row.get::<_, Option<f64>>(5)?.unwrap_or_default(),
                project_path,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads estimated cost totals for each `(source, model)` pair.
    pub fn cost_breakdown(&self, filter: &QueryFilter) -> Result<Vec<CostLine>> {
        let sql_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                source,
                model,
                SUM(input_tokens),
                SUM(cache_read_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens),
                SUM(cost_with_cache_usd),
                SUM(event_count)
            FROM usage_bucket_30m
            {}
            GROUP BY source, model
            ORDER BY SUM(total_tokens) DESC, source ASC, model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let source: String = row.get(0)?;
            let model: String = row.get(1)?;
            let total_tokens = row.get::<_, Option<i64>>(6)?.unwrap_or_default();
            let estimated_cost_usd = row.get::<_, Option<f64>>(7)?.unwrap_or_default();
            let event_count = row.get::<_, Option<i64>>(8)?.unwrap_or_default();

            Ok(CostLine {
                source,
                model,
                total_tokens,
                estimated_cost_usd,
                event_count,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads the ccr-ui home overview payload from the same dashboard connection.
    pub fn home_overview(&self, filter: &QueryFilter) -> Result<HomeOverviewPayload> {
        home_overview::load(self, filter)
    }

    /// Loads per-source archive diagnostics (F4.4 / F5.3).
    ///
    /// Reads the `source_file` state-machine counts plus
    /// `source_sync_status.{recent,history}_completed_at` columns. The
    /// completion timestamps are populated once 4.4 (RecentReady) lands;
    /// until then they are surfaced as `None`.
    pub fn diagnostics(&self) -> Result<DiagnosticsPayload> {
        let archive_root = self.store.paths.root_dir.display().to_string();
        let by_source = load_source_diagnostics(&self.conn)?;
        let recent_failures = self
            .store
            .run_log()
            .recent_runs_with_conn(&self.conn, 10)?
            .into_iter()
            .filter(crate::store::RunRecord::counts_as_failure)
            .collect();
        Ok(DiagnosticsPayload {
            archive_root,
            by_source,
            recent_failures,
        })
    }

    /// Loads a `days`-day activity heatmap (F4.3) ending today in
    /// [`QueryFilter::timezone`]. Days without activity are zero-filled so
    /// the caller renders a continuous grid; values are clamped to a
    /// 1..=366 window to bound the query.
    pub fn heatmap(&self, filter: &QueryFilter, days: u32) -> Result<Vec<HeatmapPoint>> {
        heatmap::load(self, filter, days)
    }

    /// Loads cursor-paginated usage log rows (F4.3 / D26).
    pub fn logs(&self, query: &LogsQuery) -> Result<LogsPage> {
        logs::load(self, query)
    }

    /// Loads integration, cursor, and recent failure health signals.
    pub fn health(&self) -> Result<HealthPayload> {
        let integrations = self
            .store
            .integration_state()
            .load_integration_states_with_conn(&self.conn)?;
        let recent_failures = self
            .store
            .run_log()
            .recent_runs_with_conn(&self.conn, 10)?
            .into_iter()
            .filter(crate::store::RunRecord::counts_as_failure)
            .collect::<Vec<_>>();

        let mut stmt = self.conn.prepare(
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

    /// Builds the full dashboard snapshot used by static HTML export.
    ///
    /// The snapshot still embeds the legacy four-window trends (`day`/`week`/
    /// `month`/`all`) for backwards-compat HTML export. It intentionally uses
    /// the legacy scalar trend shape because `/api/trends?window=` still
    /// exposes that contract.
    pub fn snapshot(&self, filter: &QueryFilter) -> Result<DashboardSnapshot> {
        Ok(DashboardSnapshot {
            overview: self.overview(filter)?,
            day_trends: self.trends("day", filter)?,
            week_trends: self.trends("week", filter)?,
            month_trends: self.trends("month", filter)?,
            all_trends: self.trends("all", filter)?,
            models: self.model_breakdown(filter)?,
            sources: self.source_breakdown(filter)?,
            projects: self.project_breakdown(filter)?,
            costs: self.cost_breakdown(filter)?,
            health: self.health()?,
        })
    }
}

fn query_token_summary(
    conn: &Connection,
    filter: &QueryFilter,
    cutoff: Option<&str>,
) -> Result<TokenSummary> {
    let mut sql_filter = filter.bucket_filter(None);
    if let Some(cutoff) = cutoff {
        sql_filter.push("hour_start >= ?", cutoff);
    }
    let sql = format!(
        r#"
        SELECT
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cache_read_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0)
        FROM usage_bucket_30m
        {}
        "#,
        sql_filter.where_sql()
    );

    let mut stmt = conn.prepare(&sql)?;
    Ok(stmt.query_row(
        params_from_iter(sql_filter.params().iter()),
        map_token_summary,
    )?)
}

fn query_event_count(conn: &Connection, filter: &QueryFilter, cutoff: Option<&str>) -> Result<i64> {
    let mut sql_filter = filter.bucket_filter(None);
    if let Some(cutoff) = cutoff {
        sql_filter.push("hour_start >= ?", cutoff);
    }
    let sql = format!(
        "SELECT COALESCE(SUM(event_count), 0) FROM usage_bucket_30m{}",
        sql_filter.where_sql()
    );
    scalar_i64(conn, &sql, params_from_iter(sql_filter.params().iter()))
}

fn query_cost_with_cache(
    conn: &Connection,
    filter: &QueryFilter,
    cutoff: Option<&str>,
) -> Result<f64> {
    let mut sql_filter = filter.bucket_filter(None);
    if let Some(cutoff) = cutoff {
        sql_filter.push("hour_start >= ?", cutoff);
    }
    let sql = format!(
        "SELECT COALESCE(SUM(cost_with_cache_usd), 0.0) FROM usage_bucket_30m{}",
        sql_filter.where_sql()
    );
    Ok(
        conn.query_row(&sql, params_from_iter(sql_filter.params().iter()), |row| {
            row.get::<_, f64>(0)
        })?,
    )
}

fn map_token_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<TokenSummary> {
    Ok(TokenSummary {
        input_tokens: row.get(0)?,
        cache_read_tokens: row.get(1)?,
        output_tokens: row.get(2)?,
        reasoning_output_tokens: row.get(3)?,
        total_tokens: row.get(4)?,
    })
}

/// Loads `SourceDiagnostics` rows by joining the per-source state counts in
/// `source_file` with the recent/history completion timestamps in
/// `source_sync_status`. Rows show up for any source that appears in either
/// table, sorted by source identifier.
fn load_source_diagnostics(conn: &Connection) -> Result<Vec<SourceDiagnostics>> {
    let mut stmt = conn.prepare(
        r#"
        WITH file_states AS (
            SELECT
                source,
                SUM(CASE state WHEN 'live' THEN 1 ELSE 0 END) AS live_files,
                SUM(CASE state WHEN 'missing' THEN 1 ELSE 0 END) AS missing_files,
                SUM(CASE state WHEN 'deleted_by_user' THEN 1 ELSE 0 END) AS deleted_files
            FROM source_file
            GROUP BY source
        ),
        sources AS (
            SELECT source FROM file_states
            UNION
            SELECT source FROM source_sync_status
        )
        SELECT
            s.source,
            COALESCE(fs.live_files, 0),
            COALESCE(fs.missing_files, 0),
            COALESCE(fs.deleted_files, 0),
            ss.recent_completed_at,
            ss.history_completed_at
        FROM sources s
        LEFT JOIN file_states fs ON fs.source = s.source
        LEFT JOIN source_sync_status ss ON ss.source = s.source
        ORDER BY s.source ASC
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        let source = row.get::<_, String>(0)?;
        let missing_file_count = missing_source_file_count(conn, &source)?;
        let total_events = if missing_file_count > 0 {
            source_event_count(conn, &source)?
        } else {
            0
        };
        Ok(SourceDiagnostics {
            source,
            live_files: row.get::<_, Option<i64>>(1)?.unwrap_or_default().max(0) as u64,
            missing_files: row.get::<_, Option<i64>>(2)?.unwrap_or_default().max(0) as u64,
            deleted_files: row.get::<_, Option<i64>>(3)?.unwrap_or_default().max(0) as u64,
            missing_file_count,
            protected_event_count: if missing_file_count > 0 {
                total_events
            } else {
                0
            },
            lossy_rebuild_risk: missing_file_count > 0 && total_events > 0,
            recent_completed_at: row.get(4)?,
            history_completed_at: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn source_event_count(conn: &Connection, source: &str) -> rusqlite::Result<u64> {
    Ok(conn
        .query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = ?1",
            [source],
            |row| row.get::<_, i64>(0),
        )?
        .max(0) as u64)
}

fn missing_source_file_count(conn: &Connection, source: &str) -> rusqlite::Result<u64> {
    let mut stmt = conn.prepare(
        r#"
        SELECT file_path
        FROM source_file
        WHERE source = ?1
        "#,
    )?;
    let rows = stmt.query_map([source], |row| row.get::<_, String>(0))?;
    let mut count = 0u64;
    for row in rows {
        let path = row?;
        if !std::path::Path::new(&path).exists() {
            count += 1;
        }
    }
    Ok(count)
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

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use chrono::NaiveDate;

    use super::{Dashboard, QueryFilter, ReportTimezone};
    use crate::{models::SourceKind, store::Store, testing::Fixture};

    #[test]
    fn dashboard_snapshot_uses_single_connection_and_matches_individual_methods() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(180)?;

        Store::reset_open_connection_counter();
        let dashboard = Dashboard::open(fixture.store())?;
        let snapshot = dashboard.snapshot(&Default::default())?;
        assert_eq!(Store::open_connection_count(), 1);

        let mut snapshot_overview = serde_json::to_value(&snapshot.overview)?;
        let mut method_overview = serde_json::to_value(dashboard.overview(&Default::default())?)?;
        snapshot_overview["generated_at"] = serde_json::Value::String("same".to_string());
        method_overview["generated_at"] = serde_json::Value::String("same".to_string());
        assert_eq!(snapshot_overview, method_overview);
        assert_eq!(
            serde_json::to_value(&snapshot.day_trends)?,
            serde_json::to_value(dashboard.trends("day", &Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.week_trends)?,
            serde_json::to_value(dashboard.trends("week", &Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.month_trends)?,
            serde_json::to_value(dashboard.trends("month", &Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.all_trends)?,
            serde_json::to_value(dashboard.trends("all", &Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.models)?,
            serde_json::to_value(dashboard.model_breakdown(&Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.sources)?,
            serde_json::to_value(dashboard.source_breakdown(&Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.projects)?,
            serde_json::to_value(dashboard.project_breakdown(&Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.costs)?,
            serde_json::to_value(dashboard.cost_breakdown(&Default::default())?)?
        );
        assert_eq!(
            serde_json::to_value(&snapshot.health)?,
            serde_json::to_value(dashboard.health()?)?
        );

        assert!(snapshot.overview.bucket_count >= 180);
        assert_eq!(snapshot.sources.len(), 3);
        assert!(!snapshot.models.is_empty());
        assert!(!snapshot.projects.is_empty());
        Ok(())
    }

    #[test]
    fn overview_filter_by_source_excludes_others() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(12)?;
        let dashboard = Dashboard::open(fixture.store())?;

        let all = dashboard.overview(&QueryFilter::default())?;
        let codex = dashboard.overview(&QueryFilter {
            source: Some(SourceKind::Codex),
            ..Default::default()
        })?;

        assert!(all.total.total_tokens > codex.total.total_tokens);
        assert_eq!(codex.source_count, 1);
        assert_eq!(
            dashboard
                .source_breakdown(&QueryFilter {
                    source: Some(SourceKind::Codex),
                    ..Default::default()
                })?
                .len(),
            1
        );
        Ok(())
    }

    #[test]
    fn overview_filter_by_date_range_clamps_correctly() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(72)?;
        let dashboard = Dashboard::open(fixture.store())?;

        let one_day = dashboard.overview(&QueryFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 4, 2).unwrap()),
            ..Default::default()
        })?;

        assert_eq!(one_day.bucket_count, 24);
        assert!(one_day.total.total_tokens > 0);
        assert!(
            one_day.total.total_tokens
                < dashboard.overview(&Default::default())?.total.total_tokens
        );
        Ok(())
    }

    #[test]
    fn cache_efficiency_zero_when_no_input() {
        assert_eq!(super::TokenSummary::default().cache_efficiency(), 0.0);
    }

    /// Validates the 0.5.1 ccr-ui field contract: overview, daily trends,
    /// model/project breakdowns, and logs all expose persisted cost/cache/
    /// pricing fields without requiring downstream adapters to re-SUM them.
    #[test]
    fn dashboard_ccr_ui_contract_exposes_cost_cache_and_pricing_fields() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(12)?;
        let dashboard = Dashboard::open(fixture.store())?;

        let overview = dashboard.overview(&QueryFilter::default())?;
        assert!(overview.total_cost_usd > 0.0);
        assert_eq!(overview.cache_efficiency, overview.total.cache_efficiency());

        let trend = dashboard
            .trends_daily(&QueryFilter::default())?
            .into_iter()
            .next()
            .expect("seeded trend");
        assert!(trend.cost_with_cache_usd > 0.0);

        let model = dashboard
            .model_breakdown(&QueryFilter::default())?
            .into_iter()
            .find(|row| row.model == "gpt-5")
            .expect("gpt-5 model row");
        assert!(model.cost_with_cache_usd > 0.0);
        assert!(model.cost_without_cache_usd >= model.cost_with_cache_usd);
        assert!(model.cache_savings_usd >= 0.0);
        assert_eq!(model.pricing_status, "static");
        assert_eq!(model.pricing_source.as_deref(), Some("static-v1"));
        assert!(model.pricing_rate.is_some());

        let project = dashboard
            .project_breakdown(&QueryFilter::default())?
            .into_iter()
            .next()
            .expect("seeded project row");
        assert!(project.total_cost_usd > 0.0);
        assert!(project.project_path.is_some());

        let logs = dashboard.logs(&crate::LogsQuery {
            page_size: 1,
            ..Default::default()
        })?;
        let record = logs.records.first().expect("seeded log row");
        assert_eq!(record.id, record.event_key);
        assert!(!record.recorded_at.is_empty());
        assert!(record.cost_usd >= 0.0);
        assert_eq!(record.cost_usd, record.cost_with_cache_usd);
        assert!(!record.pricing_status.is_empty());

        Ok(())
    }

    /// Validates D24/F1.4: overview surfaces an event count that matches the
    /// number of seeded usage events, and breakdown rows expose the same
    /// totals so dashboards no longer need to re-COUNT in the UI layer.
    #[test]
    fn overview_event_count_matches_row_count() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(48)?;
        let dashboard = Dashboard::open(fixture.store())?;

        let overview = dashboard.overview(&QueryFilter::default())?;
        assert_eq!(overview.total_events, 48);
        assert!(overview.total_cost_usd > 0.0);

        let model_total: i64 = dashboard
            .model_breakdown(&QueryFilter::default())?
            .iter()
            .map(|row| row.event_count)
            .sum();
        assert_eq!(model_total, 48);

        let source_total: i64 = dashboard
            .source_breakdown(&QueryFilter::default())?
            .iter()
            .map(|row| row.event_count)
            .sum();
        assert_eq!(source_total, 48);

        let cost_total: i64 = dashboard
            .cost_breakdown(&QueryFilter::default())?
            .iter()
            .map(|row| row.event_count)
            .sum();
        assert_eq!(cost_total, 48);
        Ok(())
    }

    /// Validates F4.3: a 365-day heatmap zero-fills every day in the window
    /// even when only a single bucket landed in SQLite, and surfaces the
    /// observed event_count/total_tokens on the matching local date.
    /// Validates F4.3: a 365-day heatmap zero-fills every day in the window
    /// even when only a single bucket landed in SQLite, and surfaces the
    /// observed event_count/total_tokens on the matching local date.
    #[test]
    fn heatmap_365_days_returns_all_dates_with_zero_fill() -> Result<()> {
        let fixture = Fixture::new()?;
        let conn = fixture.store().open_connection()?;
        let today_local = chrono::Local::now().date_naive();
        let target_local = today_local - chrono::Duration::days(10);
        let target_utc_midnight = format!("{}T00:00:00Z", target_local.format("%Y-%m-%d"));

        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens, event_count, updated_at
            )
            VALUES ('codex', 'gpt-5', ?1, '', NULL, NULL,
                    100, 10, 0, 50, 0, 160, 4, ?1)
            "#,
            [&target_utc_midnight],
        )?;

        let dashboard = Dashboard::open(fixture.store())?;
        let heatmap = dashboard.heatmap(&QueryFilter::default(), 365)?;
        assert_eq!(heatmap.len(), 365);

        let observed_dates: Vec<&String> = heatmap
            .iter()
            .filter(|point| point.event_count > 0)
            .map(|point| &point.date)
            .collect();
        // 跨时区时单条事件可能落在 ±1 天，因此放宽为「至少有一天观察到 4 个事件」。
        assert_eq!(observed_dates.len(), 1);

        let zero_days = heatmap
            .iter()
            .filter(|point| point.event_count == 0)
            .count();
        assert_eq!(zero_days, 364);

        let last_date = heatmap.last().expect("non-empty heatmap").date.clone();
        assert_eq!(last_date, today_local.format("%Y-%m-%d").to_string());
        Ok(())
    }
    #[test]
    fn trends_daily_groups_by_local_date_with_timezone() -> Result<()> {
        let fixture = Fixture::new()?;
        let conn = fixture.store().open_connection()?;
        // 16:00 UTC on Apr 4 = 00:00 (next day) on Apr 5 in +08:00.
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens, event_count, updated_at
            )
            VALUES ('codex', 'gpt-5', '2026-04-04T16:00:00Z', '', NULL, NULL,
                    100, 10, 5, 50, 7, 172, 2, '2026-04-05T00:00:00Z')
            "#,
            [],
        )?;
        let dashboard = Dashboard::open(fixture.store())?;

        let cn_filter = QueryFilter {
            timezone: ReportTimezone::Fixed(
                chrono::FixedOffset::east_opt(8 * 3600).expect("valid offset"),
            ),
            ..Default::default()
        };
        let cn_series = dashboard.trends_daily(&cn_filter)?;
        assert_eq!(cn_series.len(), 1);
        let row = &cn_series[0];
        assert_eq!(row.date, "2026-04-05");
        assert_eq!(row.input_tokens, 100);
        assert_eq!(row.cache_read_tokens, 10);
        assert_eq!(row.cache_creation_tokens, 5);
        assert_eq!(row.output_tokens, 57); // output(50) + reasoning(7)
        assert_eq!(row.total_tokens, 172);
        assert_eq!(row.event_count, 2);

        let utc_filter = QueryFilter {
            timezone: ReportTimezone::Utc,
            ..Default::default()
        };
        let utc_series = dashboard.trends_daily(&utc_filter)?;
        assert_eq!(utc_series.len(), 1);
        assert_eq!(utc_series[0].date, "2026-04-04");
        Ok(())
    }

    /// Validates D6/F1.3: `Store::recompute_costs` rewrites the per-event
    /// cost columns using the static-v1 catalog, so a `usage_event` seeded
    /// with zero cost now carries non-zero `cost_with_cache_usd` and a
    /// `pricing_status = 'static'` row tag.
    #[test]
    fn refresh_pricing_recomputes_all_costs() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:k1",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 1_000_000,
            cache_read_tokens: 200_000,
            output_tokens: 500_000,
            reasoning_output_tokens: 0,
            total_tokens: 1_700_000,
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        let conn = fixture.store().open_connection()?;

        let updated = fixture.store().recompute_costs()?;
        assert_eq!(updated, 1);

        let (cost_with, cost_without, status, source): (f64, f64, String, String) = conn
            .query_row(
                r#"
                SELECT cost_with_cache_usd, cost_without_cache_usd,
                       pricing_status, COALESCE(pricing_source, '')
                FROM usage_event WHERE event_key = 'codex:k1'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        assert!(cost_with > 0.0);
        assert!(cost_without > cost_with);
        assert_eq!(status, "static");
        assert_eq!(source, "static-v1");
        let (bucket_cost_with, bucket_status, bucket_source): (f64, String, String) = conn
            .query_row(
                r#"
                SELECT cost_with_cache_usd, pricing_status, COALESCE(pricing_source, '')
                FROM usage_bucket_30m
                WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-05-01T00:00:00Z'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        assert!((bucket_cost_with - cost_with).abs() < 1e-9);
        assert_eq!(bucket_status, "static");
        assert_eq!(bucket_source, "static-v1");
        Ok(())
    }

    /// Validates F1.3 snapshot path: `Store::recompute_costs_with` driven by
    /// a litellm-shaped catalog stamps `pricing_status = 'snapshot'` and
    /// the catalog's version label so dashboards can tell static vs
    /// snapshot-priced rows apart even after the same recompute pass.
    #[test]
    fn recompute_costs_with_snapshot_catalog_marks_rows_as_snapshot() -> Result<()> {
        use crate::query::PricingCatalog;
        use std::io::Write;
        use tempfile::NamedTempFile;

        let fixture = Fixture::new()?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:snap",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 500_000,
            output_tokens: 100_000,
            total_tokens: 600_000,
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        let conn = fixture.store().open_connection()?;

        let mut tmp = NamedTempFile::new()?;
        writeln!(
            tmp,
            r#"{{
                "version": "litellm-snapshot-2026-05",
                "models": [
                    {{
                        "source": "codex",
                        "matchers": ["gpt-5"],
                        "input_per_mtok": 2.0,
                        "cached_per_mtok": 0.2,
                        "output_per_mtok": 20.0
                    }}
                ]
            }}"#
        )?;
        tmp.flush()?;

        let catalog = PricingCatalog::load_snapshot(tmp.path())?;
        let updated = fixture.store().recompute_costs_with(&catalog)?;
        assert_eq!(updated, 1);

        let (status, source, cost_with): (String, String, f64) = conn.query_row(
            r#"
            SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
            FROM usage_event WHERE event_key = 'codex:snap'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(status, "snapshot");
        assert_eq!(source, "litellm-snapshot-2026-05");
        // 0.5M input @ 2.0 + 0 cache_read + 0.1M output @ 20.0 = 1.0 + 2.0 = 3.0
        assert!((cost_with - 3.0).abs() < 1e-6);
        let (bucket_status, bucket_source, bucket_cost): (String, String, f64) = conn.query_row(
            r#"
            SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
            FROM usage_bucket_30m
            WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-05-01T00:00:00Z'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(bucket_status, "snapshot");
        assert_eq!(bucket_source, "litellm-snapshot-2026-05");
        assert!((bucket_cost - 3.0).abs() < 1e-6);
        Ok(())
    }

    /// Validates C2 on the recompute path: the no-arg recompute entrypoint uses
    /// the same active catalog resolver as sync, so an active local snapshot
    /// does not diverge back to the embedded static catalog.
    #[test]
    fn recompute_costs_uses_active_pricing_catalog() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:active-snap",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 500_000,
            output_tokens: 100_000,
            total_tokens: 600_000,
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        let pricing_dir = fixture.paths().root_dir.join("pricing");
        std::fs::create_dir_all(&pricing_dir)?;
        std::fs::write(
            pricing_dir.join("litellm-snapshot-2026-05.json"),
            r#"{
                "version": "litellm-snapshot-2026-05",
                "models": [
                    {
                        "source": "codex",
                        "matchers": ["gpt-5"],
                        "input_per_mtok": 2.0,
                        "cached_per_mtok": 0.2,
                        "output_per_mtok": 20.0
                    }
                ]
            }"#,
        )?;
        fixture
            .store()
            .set_meta_value("pricing_catalog_version", "litellm-snapshot-2026-05")?;

        let updated = fixture.store().recompute_costs()?;
        assert_eq!(updated, 1);

        let conn = fixture.store().open_connection()?;
        let (status, source, cost): (String, String, f64) = conn.query_row(
            r#"
            SELECT pricing_status, COALESCE(pricing_source, ''), cost_with_cache_usd
            FROM usage_event WHERE event_key = 'codex:active-snap'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert_eq!(status, "snapshot");
        assert_eq!(source, "litellm-snapshot-2026-05");
        assert!((cost - 3.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn recompute_costs_deletes_orphan_buckets() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:live-bucket",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 1_000,
            output_tokens: 500,
            total_tokens: 1_500,
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        let conn = fixture.store().open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
                event_count, updated_at
            ) VALUES ('codex', 'gpt-5', '2026-05-02T00:00:00Z', '', NULL, NULL,
                0, 0, 0, 0, 0, 0,
                42.0, 42.0, 'static', 'static-v1', '{}',
                0, '2026-05-02T00:00:00Z')
            "#,
            [],
        )?;

        let before: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(before, 2);

        let updated = fixture.store().recompute_costs()?;
        assert_eq!(updated, 1);

        let after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(after, 1, "orphan bucket should be deleted, not zeroed");
        let orphan_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM usage_bucket_30m
            WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-05-02T00:00:00Z'
            "#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(orphan_count, 0);
        Ok(())
    }

    #[test]
    fn home_overview_includes_all_sources_in_by_platform() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(12)?;
        let payload = Dashboard::open(fixture.store())?.home_overview(&Default::default())?;

        for source in ["claude", "codex", "gemini", "opencode"] {
            assert!(payload.by_platform.contains_key(source));
        }
        assert!(payload.by_platform["codex"].requests > 0);
        assert!(payload.by_platform["claude"].requests > 0);
        assert!(payload.by_platform["opencode"].requests > 0);
        assert_eq!(payload.by_platform["gemini"].requests, 0);
        assert!(!payload.series.is_empty());
        Ok(())
    }

    #[test]
    fn home_overview_archive_by_source_empty_when_source_file_unseeded() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(3)?;
        let payload = Dashboard::open(fixture.store())?.home_overview(&Default::default())?;

        assert_eq!(
            payload.archive.archive_root,
            fixture.store().paths.root_dir.display().to_string()
        );
        assert!(payload.archive.by_source.is_empty());
        assert_eq!(payload.archive.recent_failures.len(), 1);
        Ok(())
    }

    #[test]
    fn home_overview_under_80ms_with_seeded_10k_events() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(10_000)?;
        let dashboard = Dashboard::open(fixture.store())?;
        let started = std::time::Instant::now();

        let payload = dashboard.home_overview(&Default::default())?;

        assert_eq!(payload.summary.total_requests, 10_000);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(80),
            "home_overview should stay below 80ms with 10k seeded events"
        );
        Ok(())
    }
}
