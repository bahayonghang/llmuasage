use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params_from_iter};
use serde::Serialize;

use crate::{
    domain::source_descriptor::registered_source_descriptors,
    error::Result,
    store::{IntegrationState, RunRecord, Store},
    util::now_utc,
};

mod explorer;
pub mod filter;
mod heatmap;
mod home_overview;
pub mod inventory;
pub(crate) mod logs;
pub mod pricing;
pub mod pricing_catalog;
pub mod reports;

pub use explorer::{
    ExplorerDimension, ExplorerFilters, ExplorerGranularity, ExplorerMetric, ExplorerPayload,
    ExplorerQuery, ExplorerRow, ExplorerSeriesPoint, ExplorerSupport, ExplorerTokenType,
    ExplorerTotals,
};
pub use filter::{QueryFilter, ReportTimezone};
pub use heatmap::HeatmapPoint;
pub use home_overview::{
    HomeOverviewBootstrap, HomeOverviewPayload, HomeOverviewPlatformStats, HomeOverviewSeriesItem,
    HomeOverviewSummary,
};
pub use inventory::{InstalledItem, InventoryKind, InventoryRoots, InventorySource};
pub use logs::{LogRecord, LogsPage, LogsQuery};
pub use pricing::{CostBreakdown, PRICING_MIXED, PRICING_UNPRICED, PricingStatus};
pub use pricing_catalog::PricingCatalog;

/// Aggregated token counters returned by overview and trend queries.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TokenSummary {
    /// Sum of non-cache read tokens.
    pub input_tokens: i64,
    /// Sum of cache-creation prompt tokens.
    pub cache_creation_tokens: i64,
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
        let denominator = self.input_tokens + self.cache_creation_tokens + self.cache_read_tokens;
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

/// Context-window utilization summary produced by [`Dashboard::context_pressure`].
///
/// Percentages are prompt-side occupancy (`input + cache_read + cache_creation`)
/// over the model's known maximum context window. Events whose model has no
/// known window are excluded from the ratios and counted in `unpriced_events`.
#[derive(Debug, Clone, Serialize)]
pub struct ContextPressurePayload {
    /// Highest single-event context occupancy ratio in [0, 1], if any priced.
    pub peak_percent: f64,
    /// Mean per-event context occupancy ratio in [0, 1] across priced events.
    pub avg_percent: f64,
    /// `source:model` label behind `peak_percent`, when known.
    pub peak_model: Option<String>,
    /// Events counted toward the ratios (model window known).
    pub priced_events: i64,
    /// Events skipped because the model window is unknown.
    pub unpriced_events: i64,
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
    /// Summed cache creation tokens.
    pub cache_creation_tokens: i64,
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

/// Support/degradation metadata for behavior analytics.
#[derive(Debug, Clone, Serialize)]
pub struct BehaviorSupport {
    /// Whether at least one normalized behavior row is available for the filter.
    pub supported: bool,
    /// Machine-readable source support level.
    pub level: String,
    /// Human-readable explanation suitable for empty or degraded states.
    pub reason: Option<String>,
}

/// Activity category aggregate powered by `usage_turn`.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityBreakdown {
    /// Deterministic category id, e.g. `coding` or `exploration`.
    pub category: String,
    /// Number of normalized turns in this category.
    pub turns: i64,
    /// Turns with at least one edit/write action.
    pub edit_turns: i64,
    /// Edit turns without a detected retry.
    pub one_shot_turns: i64,
    /// Sum of deterministic retry estimates.
    pub retries: i64,
    /// Number of API calls/events represented by the turns.
    pub call_count: i64,
    /// Summed tokens attributed to the turns.
    pub total_tokens: i64,
    /// Estimated cost attributed to this category by joining persisted event cost
    /// on `(source, session_id, source_path_hash, primary_model, started_at)`.
    pub estimated_cost_usd: f64,
    /// `one_shot_turns / edit_turns`, or 0 when there are no edit turns.
    pub one_shot_rate: f64,
    /// `retries / turns`, or 0 when there are no turns.
    pub retry_rate: f64,
}

/// Top-level activity analytics payload.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityPayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Category aggregates ordered by attributed cost/tokens/turns.
    pub breakdown: Vec<ActivityBreakdown>,
}

/// Tool/action aggregate powered by `usage_tool_call` plus attributed event cost.
#[derive(Debug, Clone, Serialize)]
pub struct ToolBreakdown {
    /// Coarse tool/action family.
    pub tool_kind: String,
    /// Source tool name or MCP tool name.
    pub tool_name: String,
    /// MCP server name when applicable.
    pub mcp_server: Option<String>,
    /// Number of normalized calls.
    pub calls: i64,
    /// Distinct turns touched by this tool when turn keys are available.
    pub turn_count: i64,
    /// Distinct sessions touched by this tool.
    pub session_count: i64,
    /// Estimated cost attributed through parent events after shared-event split.
    pub estimated_cost_usd: f64,
    /// Share of all calls in the current filter.
    pub call_share: f64,
    /// First observed call timestamp.
    pub first_seen_at: Option<String>,
    /// Last observed call timestamp.
    pub last_seen_at: Option<String>,
}

/// Top-level tool analytics payload.
#[derive(Debug, Clone, Serialize)]
pub struct ToolsPayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Tool aggregates ordered by calls/cost/name.
    pub breakdown: Vec<ToolBreakdown>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct AttributedToolRow {
    tool_kind: String,
    tool_name: String,
    mcp_server: Option<String>,
    calls: i64,
    turn_count: i64,
    session_count: i64,
    estimated_cost_usd: f64,
    input_tokens: f64,
    cache_read_tokens: f64,
    cache_creation_tokens: f64,
    output_tokens: f64,
    reasoning_output_tokens: f64,
    first_seen_at: Option<String>,
    last_seen_at: Option<String>,
}

/// One read-only optimization finding derived from normalized local facts.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizeFinding {
    /// Stable detector id.
    pub id: String,
    /// Human-readable finding title.
    pub title: String,
    /// `high`, `medium`, or `low`.
    pub severity: String,
    /// Evidence summary with bounded, display-safe values.
    pub evidence: String,
    /// Read-only recommendation. llmusage never executes it automatically.
    pub recommendation: String,
    /// Rough token-savings estimate; use as a prioritization hint only.
    pub estimated_savings_tokens: i64,
    /// Rough USD-savings estimate using already persisted local costs.
    pub estimated_savings_usd: f64,
}

/// Read-only behavior optimization payload.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizePayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Simple health score after detector penalties.
    pub score: i64,
    /// Letter grade derived from [`Self::score`].
    pub grade: String,
    /// Sum of detector token-savings estimates.
    pub estimated_savings_tokens: i64,
    /// Sum of detector USD-savings estimates.
    pub estimated_savings_usd: f64,
    /// Findings ordered by severity and estimated savings.
    pub findings: Vec<OptimizeFinding>,
}

/// Read-only zero-call ("zombie") inventory report: locally installed skills and
/// MCP servers that have no recorded call in `usage_tool_call`.
#[derive(Debug, Clone, Serialize)]
pub struct ZombieReport {
    /// Total installed items scanned (skills + MCP across detected sources).
    pub installed_total: usize,
    /// Installed-but-never-called items, sorted by source/kind/name.
    pub zombies: Vec<ZombieItem>,
}

/// One installed-but-never-called skill or MCP server.
#[derive(Debug, Clone, Serialize)]
pub struct ZombieItem {
    /// Owning CLI (`claude` / `codex` / `opencode`).
    pub source: String,
    /// `skill` or `mcp`.
    pub kind: String,
    /// Skill name or MCP server name.
    pub name: String,
}

/// Candidate model row for model comparison.
#[derive(Debug, Clone, Serialize)]
pub struct CompareModelCandidate {
    /// Normalized model name.
    pub model: String,
    /// Number of usage events/calls observed for the model.
    pub calls: i64,
    /// Normalized behavior turns observed for the model.
    pub turns: i64,
    /// Edit/write turns observed for the model.
    pub edit_turns: i64,
    /// Summed tokens from usage buckets.
    pub total_tokens: i64,
    /// Summed persisted cache-aware cost.
    pub estimated_cost_usd: f64,
    /// True when the sample is too small for confident behavioral comparison.
    pub low_sample: bool,
}

/// Per-model comparison statistics.
#[derive(Debug, Clone, Serialize)]
pub struct ModelCompareStats {
    /// Normalized model name.
    pub model: String,
    /// Number of usage events/calls.
    pub calls: i64,
    /// Number of normalized turns.
    pub turns: i64,
    /// Number of edit/write turns.
    pub edit_turns: i64,
    /// Number of one-shot edit/write turns.
    pub one_shot_turns: i64,
    /// Sum of deterministic retry estimates.
    pub retries: i64,
    /// Summed tokens.
    pub total_tokens: i64,
    /// Summed cache-aware cost.
    pub estimated_cost_usd: f64,
    /// Cache read ratio across persisted bucket tokens.
    pub cache_efficiency: f64,
    /// Cost per usage event/call.
    pub cost_per_call: f64,
    /// Cost per edit/write turn.
    pub cost_per_edit_turn: f64,
    /// One-shot edit/write rate.
    pub one_shot_rate: f64,
    /// Retry estimate per turn.
    pub retry_rate: f64,
    /// Average normalized tool calls per turn.
    pub avg_tools_per_turn: f64,
    /// Delegation-category turn share.
    pub delegation_rate: f64,
    /// Planning-category turn share.
    pub planning_rate: f64,
    /// True when calls or edit turns are below the comparison threshold.
    pub low_sample: bool,
}

/// Side-by-side scalar comparison metric.
#[derive(Debug, Clone, Serialize)]
pub struct CompareMetric {
    /// Stable metric id.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Value for model A.
    pub model_a_value: f64,
    /// Value for model B.
    pub model_b_value: f64,
    /// Whether a higher value is generally better for this metric.
    pub higher_is_better: bool,
}

/// Category-level one-shot comparison.
#[derive(Debug, Clone, Serialize)]
pub struct CategoryCompareRow {
    /// Activity category.
    pub category: String,
    /// Edit/write turns for model A in this category.
    pub model_a_edit_turns: i64,
    /// One-shot rate for model A in this category.
    pub model_a_one_shot_rate: f64,
    /// Edit/write turns for model B in this category.
    pub model_b_edit_turns: i64,
    /// One-shot rate for model B in this category.
    pub model_b_one_shot_rate: f64,
}

/// Model-pair comparison payload.
#[derive(Debug, Clone, Serialize)]
pub struct ModelComparePayload {
    /// Support/degradation metadata.
    pub support: BehaviorSupport,
    /// Available model candidates for the current filter.
    pub candidates: Vec<CompareModelCandidate>,
    /// Chosen left-hand model stats.
    pub model_a: Option<ModelCompareStats>,
    /// Chosen right-hand model stats.
    pub model_b: Option<ModelCompareStats>,
    /// Performance/efficiency metrics.
    pub metrics: Vec<CompareMetric>,
    /// Category head-to-head rows.
    pub category_head_to_head: Vec<CategoryCompareRow>,
    /// Working style metrics.
    pub working_style: Vec<CompareMetric>,
    /// Warning shown for no-data, insufficient models, or low-sample comparisons.
    pub warning: Option<String>,
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

/// Compact health projection used by latency-sensitive live dashboard reads.
#[derive(Debug, Clone, Serialize)]
pub struct HealthSummaryPayload {
    /// Latest install/probe states for known integrations.
    pub integrations: Vec<IntegrationState>,
    /// Number of persisted cursors without serializing every cursor key.
    pub cursor_count: i64,
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
    /// Source identifier (`codex` / `claude` / `opencode` / `antigravity`).
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

/// Top dashboard sync command-center payload. It answers ordinary sync safety
/// separately from lossy rebuild risk and exposes only structured facts.
#[derive(Debug, Clone, Serialize)]
pub struct SyncCommandCenterPayload {
    pub mode: String,
    pub tone: String,
    pub headline_key: String,
    pub reason_key: String,
    pub generated_at: String,
    pub current_job: Option<SyncCurrentJobPayload>,
    pub last_run: Option<SyncLastRunPayload>,
    pub safety: SyncSafetyPayload,
    pub metrics: SyncMetricsPayload,
    pub sources: Vec<SyncSourcePayload>,
    pub actions: Vec<SyncActionPayload>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncCurrentJobPayload {
    pub job_id: String,
    pub status: String,
    pub last_event: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncLastRunPayload {
    pub status: String,
    pub command: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSafetyPayload {
    pub ordinary_sync_safe: bool,
    pub worker_lock: String,
    pub worker_lock_holder: Option<String>,
    pub lossy_rebuild_risk: bool,
    pub risk_sources: Vec<String>,
    pub recent_failures: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMetricsPayload {
    pub events_seen: i64,
    pub inserted_delta: i64,
    pub stored_events: i64,
    pub sources_ready: i64,
    pub sources_total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncSourcePayload {
    pub source: String,
    pub status: String,
    pub tone: String,
    pub files_processed: i64,
    pub changed_files: i64,
    pub skipped_files: i64,
    pub events_seen: i64,
    pub events_inserted: i64,
    pub stored_events: i64,
    pub updated_at: Option<String>,
    pub share: f64,
    pub error_key: Option<String>,
    pub lossy_rebuild_risk: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncActionPayload {
    pub id: String,
    pub label_key: String,
    pub primary: bool,
    pub disabled: bool,
    pub reason_key: Option<String>,
}

/// Full snapshot embedded into exported HTML bundles.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardSnapshot {
    /// Headline overview metrics.
    pub overview: OverviewPayload,
    /// Structured top-of-page sync safety and latest-run summary.
    pub sync_command_center: SyncCommandCenterPayload,
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
    /// Behavior activity categories. Empty with `support.supported=false` when
    /// the database has no normalized turn facts for the current filter.
    pub activity: ActivityPayload,
    /// Tool/action breakdowns. Empty with `support.supported=false` when the
    /// database has no normalized tool facts for the current filter.
    pub tools: ToolsPayload,
    /// Read-only behavior optimization findings. Empty/degraded when behavior
    /// facts are unavailable.
    pub optimize: OptimizePayload,
    /// Default model comparison payload. If fewer than two models are present
    /// it carries candidates plus an explicit warning.
    pub compare: ModelComparePayload,
    /// Default Cost Explorer slice captured for live dashboard bootstrap and
    /// static HTML exports.
    pub explorer: ExplorerPayload,
    /// Integration/cursor/run health payload.
    pub health: HealthPayload,
    /// Archive/source-file diagnostics plus recent failed run records.
    pub diagnostics: DiagnosticsPayload,
}

/// Dashboard snapshot core sections that must stay responsive even when
/// behavior analytics degrades.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardCoreSnapshot {
    /// Top-level totals and recent status.
    pub overview: OverviewPayload,
    /// Structured top-of-page sync safety and latest-run summary.
    pub sync_command_center: SyncCommandCenterPayload,
    /// 24h-style trend rows.
    pub day_trends: Vec<TrendPoint>,
    /// 7d-style trend rows.
    pub week_trends: Vec<TrendPoint>,
    /// 30d-style trend rows.
    pub month_trends: Vec<TrendPoint>,
    /// All-time trend rows.
    pub all_trends: Vec<TrendPoint>,
    /// Per-model cost/token table.
    pub models: Vec<ModelBreakdown>,
    /// Per-source cost/token table.
    pub sources: Vec<SourceBreakdown>,
    /// Per-project cost/token table.
    pub projects: Vec<ProjectBreakdown>,
    /// Per-source/model cost estimate table.
    pub costs: Vec<CostLine>,
    /// Integration/cursor/run health payload.
    pub health: HealthPayload,
    /// Archive/source-file diagnostics plus recent failed run records.
    pub diagnostics: DiagnosticsPayload,
}

/// Lean live-dashboard projection for one selected time range.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardInteractiveSnapshot {
    pub overview: OverviewPayload,
    pub sync_command_center: SyncCommandCenterPayload,
    pub trends: Vec<TrendPoint>,
    pub models: Vec<ModelBreakdown>,
    pub sources: Vec<SourceBreakdown>,
    pub projects: Vec<ProjectBreakdown>,
    pub costs: Vec<CostLine>,
    pub health: HealthSummaryPayload,
    pub diagnostics: DiagnosticsPayload,
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

    /// Opens a Dashboard whose connection uses a shorter `busy_timeout`.
    ///
    /// Web/API handlers use this so a locked database surfaces as a fast
    /// section error inside the existing timeout/degraded flow instead of
    /// blocking for the writer-oriented 30s default. Sync writers and export
    /// paths keep using [`Dashboard::open`].
    pub fn open_with_busy_timeout(
        store: &Store,
        busy_timeout: std::time::Duration,
    ) -> Result<Self> {
        let conn = store.open_connection_with_busy_timeout(busy_timeout)?;
        Ok(Self {
            store: store.clone(),
            conn,
        })
    }

    pub(crate) fn interrupt_handle(&self) -> rusqlite::InterruptHandle {
        self.conn.get_interrupt_handle()
    }

    #[cfg(test)]
    pub(crate) fn test_slow_query(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "WITH RECURSIVE counter(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM counter WHERE value < 100000000) SELECT SUM(value) FROM counter",
            [],
            |row| row.get(0),
        )?)
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
            "day" | "hourly" => "hour_start".to_string(),
            "week" | "month" => format!("date(hour_start, '{modifier}')"),
            _ => format!("strftime('%Y-%m', hour_start, '{modifier}')"),
        };
        let sql = format!(
            r#"
            SELECT {label_expr} AS label,
                   COALESCE(SUM(total_tokens), 0) AS total_tokens
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
    /// (D9/F4.2). Reasoning remains a separate diagnostic channel.
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
                COALESCE(SUM(output_tokens), 0),
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
                SUM(cache_creation_tokens),
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
            ORDER BY
                SUM(total_tokens) DESC,
                model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let cost_with_cache_usd = row.get::<_, Option<f64>>(8)?.unwrap_or_default();
            let cost_without_cache_usd = row.get::<_, Option<f64>>(9)?.unwrap_or_default();
            Ok(ModelBreakdown {
                model: row.get(0)?,
                input_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                cache_creation_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                cache_read_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                reasoning_output_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                event_count: row.get::<_, Option<i64>>(7)?.unwrap_or_default(),
                cost_with_cache_usd,
                cost_without_cache_usd,
                cache_savings_usd: (cost_without_cache_usd - cost_with_cache_usd).max(0.0),
                pricing_status: row
                    .get::<_, Option<String>>(10)?
                    .unwrap_or_else(|| PRICING_UNPRICED.to_string()),
                pricing_source: row.get(11)?,
                pricing_rate: row.get(12)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Summarizes context-window utilization across the filtered event set.
    ///
    /// Grouped by `(source, model)` to avoid per-event scans: each group's peak
    /// prompt tokens and summed prompt tokens are divided by the model's known
    /// context window (from the static catalog). Groups whose model window is
    /// unknown are excluded from the ratios and reported as `unpriced_events`.
    pub fn context_pressure(&self, filter: &QueryFilter) -> Result<ContextPressurePayload> {
        let event_filter = context_pressure_event_filter(filter);
        let sql = format!(
            r#"
            SELECT
                source,
                model,
                MAX(input_tokens + cache_read_tokens + cache_creation_tokens) AS peak_prompt,
                SUM(input_tokens + cache_read_tokens + cache_creation_tokens) AS sum_prompt,
                COUNT(*) AS event_count
            FROM usage_event
            {}
            GROUP BY source, model
            "#,
            event_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(event_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let catalog = self.store.active_pricing_catalog()?;
        let mut peak_percent = 0.0_f64;
        let mut peak_model: Option<String> = None;
        let mut ratio_sum = 0.0_f64;
        let mut priced_events = 0_i64;
        let mut unpriced_events = 0_i64;
        for (source, model, peak_prompt, sum_prompt, event_count) in rows {
            match catalog.context_window(&source, &model) {
                Some(window) => {
                    let window = window as f64;
                    let group_peak = peak_prompt.max(0) as f64 / window;
                    if group_peak > peak_percent {
                        peak_percent = group_peak;
                        peak_model = Some(format!("{source}:{model}"));
                    }
                    ratio_sum += sum_prompt.max(0) as f64 / window;
                    priced_events += event_count;
                }
                None => unpriced_events += event_count,
            }
        }
        let avg_percent = if priced_events > 0 {
            ratio_sum / priced_events as f64
        } else {
            0.0
        };
        Ok(ContextPressurePayload {
            peak_percent,
            avg_percent,
            peak_model,
            priced_events,
            unpriced_events,
        })
    }

    /// Loads recent 5-hour rolling blocks (burn rate / projection) for the
    /// interactive dashboard, reusing the CLI `blocks` report engine with
    /// dashboard-friendly defaults (recent blocks, local time, 5h windows).
    pub fn blocks_report(&self) -> anyhow::Result<Vec<reports::BlockReportRow>> {
        let filter = reports::ReportFilter {
            since: None,
            until: None,
            order: reports::SortOrder::Desc,
            timezone: reports::ReportTimezone::Local,
            locale: "en-US".to_string(),
            source: None,
            project: None,
            breakdown: false,
        };
        let options = reports::BlockReportOptions {
            active_only: false,
            recent_only: true,
            token_limit: None,
            session_length_hours: 5.0,
        };
        Ok(reports::load_blocks_report(&self.store, &filter, &options)?.blocks)
    }

    /// Loads total token usage grouped by source plus each source's freshest event time.
    pub fn source_breakdown(&self, filter: &QueryFilter) -> Result<Vec<SourceBreakdown>> {
        let bucket_filter = filter.bucket_filter(None);
        let sql = format!(
            r#"
            SELECT
                source,
                SUM(total_tokens) AS total_tokens,
                SUM(event_count) AS event_count
            FROM usage_bucket_30m
            {}
            GROUP BY source
            ORDER BY total_tokens DESC, source ASC
            "#,
            bucket_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(bucket_filter.params().iter()), |row| {
            Ok(SourceBreakdown {
                source: row.get(0)?,
                total_tokens: row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                last_event_at: None,
                event_count: row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
            })
        })?;
        let mut sources = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        for source in &mut sources {
            let mut event_filter = filter.event_filter(None);
            event_filter.push("source = ?", source.source.clone());
            let last_event_sql = format!(
                "SELECT MAX(event_at) FROM usage_event {}",
                event_filter.where_sql()
            );
            source.last_event_at = self.conn.query_row(
                &last_event_sql,
                params_from_iter(event_filter.params().iter()),
                |row| row.get(0),
            )?;
        }

        Ok(sources)
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
            ORDER BY
                SUM(total_tokens) DESC,
                MAX(project_label) ASC
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
                SUM(cache_creation_tokens),
                SUM(cache_read_tokens),
                SUM(output_tokens),
                SUM(reasoning_output_tokens),
                SUM(total_tokens),
                SUM(cost_with_cache_usd),
                SUM(event_count)
            FROM usage_bucket_30m
            {}
            GROUP BY source, model
            ORDER BY
                SUM(total_tokens) DESC,
                source ASC,
                model ASC
            "#,
            sql_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sql_filter.params().iter()), |row| {
            let source: String = row.get(0)?;
            let model: String = row.get(1)?;
            let total_tokens = row.get::<_, Option<i64>>(7)?.unwrap_or_default();
            let estimated_cost_usd = row.get::<_, Option<f64>>(8)?.unwrap_or_default();
            let event_count = row.get::<_, Option<i64>>(9)?.unwrap_or_default();

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

    /// Loads activity category aggregates from normalized `usage_turn` facts.
    ///
    /// This intentionally does not read raw JSONL or frontend-owned data. Cost
    /// is attribution-only: the query joins persisted `usage_event` cost rows
    /// that match the conservative one-event turn identity.
    pub fn activity_breakdown(&self, filter: &QueryFilter) -> Result<ActivityPayload> {
        let turn_filter = filter.turn_filter(Some("t"));
        let support = behavior_support(&self.conn, "usage_turn", filter.turn_filter(None))?;
        let sql = format!(
            r#"
            SELECT
                t.category,
                COUNT(*) AS turns,
                COALESCE(SUM(t.has_edits), 0) AS edit_turns,
                COALESCE(SUM(t.one_shot), 0) AS one_shot_turns,
                COALESCE(SUM(t.retries), 0) AS retries,
                COALESCE(SUM(t.call_count), 0) AS call_count,
                COALESCE(SUM(t.total_tokens), 0) AS total_tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS estimated_cost_usd
            FROM usage_turn t
            LEFT JOIN usage_event e
                ON e.event_key = substr(t.turn_key, 6)
            {}
            GROUP BY t.category
            ORDER BY estimated_cost_usd DESC, total_tokens DESC, turns DESC, t.category ASC
            "#,
            turn_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
            let turns = row.get::<_, Option<i64>>(1)?.unwrap_or_default();
            let edit_turns = row.get::<_, Option<i64>>(2)?.unwrap_or_default();
            let one_shot_turns = row.get::<_, Option<i64>>(3)?.unwrap_or_default();
            let retries = row.get::<_, Option<i64>>(4)?.unwrap_or_default();
            Ok(ActivityBreakdown {
                category: row.get(0)?,
                turns,
                edit_turns,
                one_shot_turns,
                retries,
                call_count: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                total_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or_default(),
                estimated_cost_usd: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                one_shot_rate: ratio(one_shot_turns, edit_turns),
                retry_rate: ratio(retries, turns),
            })
        })?;
        Ok(ActivityPayload {
            support,
            breakdown: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        })
    }

    /// Loads attributed tool/action aggregates from normalized behavior facts.
    ///
    /// Shared-event cost is split across sibling tool calls, and cost-bearing
    /// turns without any tool calls are surfaced as a `(non-tool)` bucket.
    pub fn tool_breakdown(&self, filter: &QueryFilter) -> Result<ToolsPayload> {
        let support = behavior_support(&self.conn, "usage_event", filter.event_filter(None))?;
        if !support.supported {
            return Ok(ToolsPayload {
                support,
                breakdown: Vec::new(),
            });
        }

        let rows = self.tool_attribution_rows(filter)?;
        let total_calls: i64 = rows.iter().map(|row| row.calls).sum();
        let breakdown = rows
            .into_iter()
            .map(|row| ToolBreakdown {
                tool_kind: row.tool_kind,
                tool_name: row.tool_name,
                mcp_server: row.mcp_server,
                calls: row.calls,
                turn_count: row.turn_count,
                session_count: row.session_count,
                estimated_cost_usd: row.estimated_cost_usd,
                call_share: ratio(row.calls, total_calls),
                first_seen_at: row.first_seen_at,
                last_seen_at: row.last_seen_at,
            })
            .collect();
        Ok(ToolsPayload { support, breakdown })
    }

    fn tool_attribution_rows(&self, filter: &QueryFilter) -> Result<Vec<AttributedToolRow>> {
        let event_filter = filter.event_filter(Some("e"));
        let tool_filter = filter.tool_filter(Some("tc"));
        let sql = format!(
            r#"
            WITH filtered_events AS (
                SELECT
                    e.event_key,
                    e.event_at,
                    e.session_id,
                    COALESCE(e.cost_with_cache_usd, 0.0) AS cost_with_cache_usd,
                    COALESCE(e.input_tokens, 0) AS input_tokens,
                    COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                    COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                    COALESCE(e.output_tokens, 0) AS output_tokens,
                    COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens
                FROM usage_event e
                {event_where}
            ),
            filtered_tools AS (
                SELECT
                    tc.tool_call_key,
                    tc.event_key,
                    tc.turn_key,
                    tc.session_id,
                    tc.occurred_at,
                    tc.tool_kind,
                    tc.tool_name,
                    tc.mcp_server
                FROM usage_tool_call tc
                {tool_where}
            ),
            event_tool_counts AS (
                SELECT
                    tc.event_key,
                    COUNT(*) AS tool_count
                FROM filtered_tools tc
                WHERE tc.event_key IS NOT NULL
                GROUP BY tc.event_key
            ),
            attributed_rows AS (
                SELECT
                    tc.tool_kind AS tool_kind,
                    tc.tool_name AS tool_name,
                    tc.mcp_server AS mcp_server,
                    COALESCE(tc.turn_key, 'turn:' || tc.event_key) AS turn_key,
                    COALESCE(tc.session_id, e.session_id) AS session_id,
                    tc.occurred_at AS occurred_at,
                    1 AS call_count,
                    COALESCE(e.cost_with_cache_usd, 0.0) / ec.tool_count AS estimated_cost_usd,
                    COALESCE(e.input_tokens, 0) * (1.0 / ec.tool_count) AS input_tokens,
                    COALESCE(e.cache_read_tokens, 0) * (1.0 / ec.tool_count) AS cache_read_tokens,
                    COALESCE(e.cache_creation_tokens, 0) * (1.0 / ec.tool_count) AS cache_creation_tokens,
                    COALESCE(e.output_tokens, 0) * (1.0 / ec.tool_count) AS output_tokens,
                    COALESCE(e.reasoning_output_tokens, 0) * (1.0 / ec.tool_count) AS reasoning_output_tokens
                FROM filtered_tools tc
                JOIN usage_event e ON e.event_key = tc.event_key
                JOIN event_tool_counts ec ON ec.event_key = tc.event_key

                UNION ALL

                SELECT
                    '(non-tool)' AS tool_kind,
                    '(non-tool)' AS tool_name,
                    NULL AS mcp_server,
                    'turn:' || e.event_key AS turn_key,
                    e.session_id AS session_id,
                    e.event_at AS occurred_at,
                    0 AS call_count,
                    COALESCE(e.cost_with_cache_usd, 0.0) AS estimated_cost_usd,
                    COALESCE(e.input_tokens, 0) AS input_tokens,
                    COALESCE(e.cache_read_tokens, 0) AS cache_read_tokens,
                    COALESCE(e.cache_creation_tokens, 0) AS cache_creation_tokens,
                    COALESCE(e.output_tokens, 0) AS output_tokens,
                    COALESCE(e.reasoning_output_tokens, 0) AS reasoning_output_tokens
                FROM filtered_events e
                LEFT JOIN filtered_tools tc ON tc.event_key = e.event_key
                WHERE tc.tool_call_key IS NULL
            )
            SELECT
                tool_kind,
                tool_name,
                mcp_server,
                COALESCE(SUM(call_count), 0) AS calls,
                COUNT(DISTINCT turn_key) AS turn_count,
                COUNT(DISTINCT session_id) AS session_count,
                COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
                COALESCE(SUM(input_tokens), 0.0) AS input_tokens,
                COALESCE(SUM(cache_read_tokens), 0.0) AS cache_read_tokens,
                COALESCE(SUM(cache_creation_tokens), 0.0) AS cache_creation_tokens,
                COALESCE(SUM(output_tokens), 0.0) AS output_tokens,
                COALESCE(SUM(reasoning_output_tokens), 0.0) AS reasoning_output_tokens,
                MIN(occurred_at) AS first_seen_at,
                MAX(occurred_at) AS last_seen_at
            FROM attributed_rows
            GROUP BY tool_kind, tool_name, mcp_server
            ORDER BY calls DESC, estimated_cost_usd DESC, tool_kind ASC, tool_name ASC
            LIMIT 50
            "#,
            event_where = event_filter.where_sql(),
            tool_where = tool_filter.where_sql()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params_from_iter(
                event_filter
                    .params()
                    .iter()
                    .chain(tool_filter.params().iter()),
            ),
            |row| {
                Ok(AttributedToolRow {
                    tool_kind: row.get(0)?,
                    tool_name: row.get(1)?,
                    mcp_server: row.get(2)?,
                    calls: row.get::<_, Option<i64>>(3)?.unwrap_or_default(),
                    turn_count: row.get::<_, Option<i64>>(4)?.unwrap_or_default(),
                    session_count: row.get::<_, Option<i64>>(5)?.unwrap_or_default(),
                    estimated_cost_usd: row.get::<_, Option<f64>>(6)?.unwrap_or_default(),
                    input_tokens: row.get::<_, Option<f64>>(7)?.unwrap_or_default(),
                    cache_read_tokens: row.get::<_, Option<f64>>(8)?.unwrap_or_default(),
                    cache_creation_tokens: row.get::<_, Option<f64>>(9)?.unwrap_or_default(),
                    output_tokens: row.get::<_, Option<f64>>(10)?.unwrap_or_default(),
                    reasoning_output_tokens: row.get::<_, Option<f64>>(11)?.unwrap_or_default(),
                    first_seen_at: row.get(12)?,
                    last_seen_at: row.get(13)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Loads read-only behavior optimization findings.
    ///
    /// The detectors are intentionally conservative and only use normalized
    /// `usage_turn` / `usage_tool_call` rows plus persisted event costs. They
    /// never inspect raw transcripts and never execute cleanup actions.
    pub fn optimize(&self, filter: &QueryFilter) -> Result<OptimizePayload> {
        let support = behavior_support(&self.conn, "usage_turn", filter.turn_filter(None))?;
        if !support.supported {
            return Ok(OptimizePayload {
                support,
                score: 100,
                grade: "A".to_string(),
                estimated_savings_tokens: 0,
                estimated_savings_usd: 0.0,
                findings: Vec::new(),
            });
        }

        let mut findings = Vec::new();
        if let Some(finding) = self.detect_low_read_edit_ratio(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_duplicate_reads(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_junk_reads(filter)? {
            findings.push(finding);
        }
        if let Some(finding) = self.detect_session_outlier(filter)? {
            findings.push(finding);
        }

        findings.sort_by(|left, right| {
            severity_rank(&right.severity)
                .cmp(&severity_rank(&left.severity))
                .then_with(|| {
                    right
                        .estimated_savings_tokens
                        .cmp(&left.estimated_savings_tokens)
                })
                .then_with(|| left.id.cmp(&right.id))
        });
        let estimated_savings_tokens = findings
            .iter()
            .map(|finding| finding.estimated_savings_tokens)
            .sum();
        let estimated_savings_usd = findings
            .iter()
            .map(|finding| finding.estimated_savings_usd)
            .sum();
        let penalty = findings
            .iter()
            .map(|finding| match finding.severity.as_str() {
                "high" => 25,
                "medium" => 15,
                _ => 7,
            })
            .sum::<i64>();
        let score = (100 - penalty).clamp(0, 100);

        Ok(OptimizePayload {
            support,
            score,
            grade: health_grade(score).to_string(),
            estimated_savings_tokens,
            estimated_savings_usd,
            findings,
        })
    }

    /// Diffs locally-installed skills / MCP servers against the actually-called
    /// set in `usage_tool_call`, returning never-called ("zombie") candidates.
    ///
    /// Read-only: this only reports candidates; llmusage never deletes or modifies
    /// anything. Matching is by `(source, name)` — skills resolve to concrete names
    /// only for Claude and OpenCode (Codex skills are not scanned), MCP matches by
    /// `(source, server)` across all three CLIs.
    pub fn zombie_report(&self, roots: &inventory::InventoryRoots) -> Result<ZombieReport> {
        let installed = roots.scan();
        let used_skills = self.used_tool_pairs("skill", "tool_name")?;
        let used_mcp = self.used_tool_pairs("mcp", "mcp_server")?;

        let mut zombies = Vec::new();
        for item in &installed {
            let used = match item.kind {
                inventory::InventoryKind::Skill => &used_skills,
                inventory::InventoryKind::Mcp => &used_mcp,
            };
            let key = (item.source.as_str().to_string(), item.name.clone());
            if !used.contains(&key) {
                zombies.push(ZombieItem {
                    source: item.source.as_str().to_string(),
                    kind: item.kind.as_str().to_string(),
                    name: item.name.clone(),
                });
            }
        }
        Ok(ZombieReport {
            installed_total: installed.len(),
            zombies,
        })
    }

    /// Distinct `(source, value)` pairs actually observed for a `tool_kind`.
    /// `column` is a fixed identifier (`tool_name` / `mcp_server`), never user input.
    fn used_tool_pairs(&self, tool_kind: &str, column: &str) -> Result<BTreeSet<(String, String)>> {
        let sql = format!(
            "SELECT DISTINCT source, {column} FROM usage_tool_call \
             WHERE tool_kind = ?1 AND {column} IS NOT NULL AND {column} != ''"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map([tool_kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut set = BTreeSet::new();
        for row in rows {
            set.insert(row?);
        }
        Ok(set)
    }

    fn detect_low_read_edit_ratio(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let tool_filter = filter.tool_filter(Some("tc"));
        let sql = format!(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN tc.tool_kind IN ('read', 'search') THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN e.total_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN tc.tool_kind = 'edit' THEN e.cost_with_cache_usd ELSE 0.0 END), 0.0)
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            "#,
            tool_filter.where_sql()
        );
        let (read_calls, edit_calls, edit_tokens, edit_cost): (i64, i64, i64, f64) = self
            .conn
            .query_row(&sql, params_from_iter(tool_filter.params().iter()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
        if edit_calls < 3 {
            return Ok(None);
        }
        let read_edit_ratio = read_calls as f64 / edit_calls as f64;
        if read_edit_ratio >= 0.5 {
            return Ok(None);
        }
        Ok(Some(OptimizeFinding {
            id: "low_read_edit_ratio".to_string(),
            title: "Low Read/Edit ratio".to_string(),
            severity: if read_edit_ratio < 0.25 {
                "high"
            } else {
                "medium"
            }
            .to_string(),
            evidence: format!(
                "{read_calls} read/search calls for {edit_calls} edit calls in this filter."
            ),
            recommendation:
                "Review files before larger edit runs; this is a read-only signal, not an automatic rewrite."
                    .to_string(),
            estimated_savings_tokens: (edit_tokens / 5).max(0),
            estimated_savings_usd: (edit_cost * 0.20).max(0.0),
        }))
    }

    fn detect_duplicate_reads(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let mut tool_filter = filter.tool_filter(Some("tc"));
        tool_filter.push_raw("tc.tool_kind IN ('read', 'search')");
        tool_filter.push_raw("tc.session_id IS NOT NULL");
        let sql = format!(
            r#"
            SELECT
                tc.session_id,
                COALESCE(tc.input_fingerprint, tc.safe_preview, tc.tool_name) AS target,
                COUNT(*) AS calls,
                COALESCE(SUM(e.total_tokens), 0) AS tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            GROUP BY tc.session_id, target
            HAVING calls > 1
            ORDER BY calls DESC, tokens DESC
            LIMIT 1
            "#,
            tool_filter.where_sql()
        );
        let row = self
            .conn
            .query_row(&sql, params_from_iter(tool_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, f64>(4)?,
                ))
            })
            .optional()?;
        let Some((session_id, target, calls, tokens, cost)) = row else {
            return Ok(None);
        };
        Ok(Some(OptimizeFinding {
            id: "duplicate_reads".to_string(),
            title: "Repeated reads in one session".to_string(),
            severity: if calls >= 5 { "high" } else { "medium" }.to_string(),
            evidence: format!(
                "Session {session_id} read/search target `{}` {calls} times.",
                safe_short(&target, 72)
            ),
            recommendation:
                "Cache the relevant facts in notes or inspect a narrower range before rereading the same target."
                    .to_string(),
            estimated_savings_tokens: (tokens * (calls - 1) / calls).max(0),
            estimated_savings_usd: (cost * (calls - 1) as f64 / calls as f64).max(0.0),
        }))
    }

    fn detect_junk_reads(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let mut tool_filter = filter.tool_filter(Some("tc"));
        tool_filter.push_raw(
            r#"
            tc.tool_kind IN ('read', 'search')
            AND (
                LOWER(COALESCE(tc.safe_preview, '')) LIKE '%node_modules%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/target/%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\target\%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/dist/%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\dist\%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%/build/%'
                OR LOWER(COALESCE(tc.safe_preview, '')) LIKE '%\build\%'
            )
            "#,
        );
        let sql = format!(
            r#"
            SELECT
                COUNT(*) AS calls,
                COALESCE(SUM(e.total_tokens), 0) AS tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost,
                MAX(COALESCE(tc.safe_preview, tc.tool_name)) AS example
            FROM usage_tool_call tc
            LEFT JOIN usage_event e ON e.event_key = tc.event_key
            {}
            "#,
            tool_filter.where_sql()
        );
        let (calls, tokens, cost, example): (i64, i64, f64, Option<String>) =
            self.conn
                .query_row(&sql, params_from_iter(tool_filter.params().iter()), |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })?;
        if calls == 0 {
            return Ok(None);
        }
        Ok(Some(OptimizeFinding {
            id: "junk_reads".to_string(),
            title: "Generated or dependency reads".to_string(),
            severity: if calls >= 5 { "high" } else { "low" }.to_string(),
            evidence: format!(
                "{calls} read/search calls touched generated or dependency-looking paths; example `{}`.",
                safe_short(example.as_deref().unwrap_or("--"), 72)
            ),
            recommendation:
                "Prefer source directories and ignore generated/dependency folders in manual investigation."
                    .to_string(),
            estimated_savings_tokens: (tokens / 2).max(0),
            estimated_savings_usd: (cost * 0.50).max(0.0),
        }))
    }

    fn detect_session_outlier(&self, filter: &QueryFilter) -> Result<Option<OptimizeFinding>> {
        let turn_filter = filter.turn_filter(Some("t"));
        let sql = format!(
            r#"
            SELECT
                t.session_id,
                COUNT(*) AS turns,
                COALESCE(SUM(t.total_tokens), 0) AS tokens,
                COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
            FROM usage_turn t
            LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
            {}
            GROUP BY t.session_id
            HAVING t.session_id IS NOT NULL
            ORDER BY tokens DESC
            LIMIT 1
            "#,
            turn_filter.where_sql()
        );
        let top = self
            .conn
            .query_row(&sql, params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })
            .optional()?;
        let Some((session_id, turns, tokens, cost)) = top else {
            return Ok(None);
        };
        let total_tokens = scalar_i64(
            &self.conn,
            &format!(
                "SELECT COALESCE(SUM(t.total_tokens), 0) FROM usage_turn t{}",
                turn_filter.where_sql()
            ),
            params_from_iter(turn_filter.params().iter()),
        )?;
        if total_tokens <= 0 || tokens * 100 / total_tokens < 40 || turns < 3 {
            return Ok(None);
        }
        Ok(Some(OptimizeFinding {
            id: "session_outlier".to_string(),
            title: "One session dominates behavior cost".to_string(),
            severity: "medium".to_string(),
            evidence: format!(
                "Session {session_id} accounts for {:.1}% of turn tokens in this filter.",
                tokens as f64 * 100.0 / total_tokens as f64
            ),
            recommendation:
                "Inspect this session before optimizing globally; long context or repeated retries may be local to it."
                    .to_string(),
            estimated_savings_tokens: (tokens / 4).max(0),
            estimated_savings_usd: (cost * 0.25).max(0.0),
        }))
    }

    /// Loads model candidates for behavior comparison.
    pub fn compare_models(&self, filter: &QueryFilter) -> Result<Vec<CompareModelCandidate>> {
        compare_model_candidates(&self.conn, filter)
    }

    /// Loads a model-pair comparison. When either model is omitted, the top two
    /// candidates in the current filter are chosen automatically.
    pub fn model_compare(
        &self,
        filter: &QueryFilter,
        model_a: Option<&str>,
        model_b: Option<&str>,
    ) -> Result<ModelComparePayload> {
        let candidates = self.compare_models(filter)?;
        if candidates.len() < 2 {
            return Ok(ModelComparePayload {
                support: BehaviorSupport {
                    supported: false,
                    level: "insufficient_models".to_string(),
                    reason: Some(
                        "At least two models with local usage are required for comparison."
                            .to_string(),
                    ),
                },
                candidates,
                model_a: None,
                model_b: None,
                metrics: Vec::new(),
                category_head_to_head: Vec::new(),
                working_style: Vec::new(),
                warning: Some("Need at least two models in the current filter.".to_string()),
            });
        }

        let selected_a = model_a
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(candidates[0].model.as_str());
        let selected_b = model_b
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                candidates
                    .iter()
                    .find(|candidate| candidate.model != selected_a)
                    .map(|candidate| candidate.model.as_str())
                    .unwrap_or(candidates[1].model.as_str())
            });

        let stats_a = self.model_compare_stats(filter, selected_a)?;
        let stats_b = self.model_compare_stats(filter, selected_b)?;
        let (support, warning) = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => {
                let warning = if left.low_sample || right.low_sample {
                    Some(
                        "Low sample: compare directionally until each model has more calls/edit turns."
                            .to_string(),
                    )
                } else {
                    None
                };
                (
                    BehaviorSupport {
                        supported: true,
                        level: if warning.is_some() {
                            "low_sample"
                        } else {
                            "normalized"
                        }
                        .to_string(),
                        reason: warning.clone(),
                    },
                    warning,
                )
            }
            _ => (
                BehaviorSupport {
                    supported: false,
                    level: "missing_model".to_string(),
                    reason: Some("One selected model has no data in this filter.".to_string()),
                },
                Some("One selected model has no data in this filter.".to_string()),
            ),
        };

        let metrics = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => compare_metrics(left, right),
            _ => Vec::new(),
        };
        let working_style = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => working_style_metrics(left, right),
            _ => Vec::new(),
        };
        let category_head_to_head = match (&stats_a, &stats_b) {
            (Some(left), Some(right)) => {
                self.category_compare(filter, &left.model, &right.model)?
            }
            _ => Vec::new(),
        };

        Ok(ModelComparePayload {
            support,
            candidates,
            model_a: stats_a,
            model_b: stats_b,
            metrics,
            category_head_to_head,
            working_style,
            warning,
        })
    }

    fn model_compare_stats(
        &self,
        filter: &QueryFilter,
        model: &str,
    ) -> Result<Option<ModelCompareStats>> {
        let mut model_filter = filter.clone();
        model_filter.model = Some(model.to_string());

        let bucket_filter = model_filter.bucket_filter(Some("b"));
        let token_sql = format!(
            r#"
            SELECT
                COALESCE(SUM(b.event_count), 0),
                COALESCE(SUM(b.total_tokens), 0),
                COALESCE(SUM(b.cost_with_cache_usd), 0.0),
                COALESCE(SUM(b.input_tokens), 0),
                COALESCE(SUM(b.cache_creation_tokens), 0),
                COALESCE(SUM(b.cache_read_tokens), 0)
            FROM usage_bucket_30m b
            {}
            "#,
            bucket_filter.where_sql()
        );
        let (calls, total_tokens, cost, input, cache_creation, cache_read): (
            i64,
            i64,
            f64,
            i64,
            i64,
            i64,
        ) = self.conn.query_row(
            &token_sql,
            params_from_iter(bucket_filter.params().iter()),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;
        if calls == 0 {
            return Ok(None);
        }

        let turn_filter = model_filter.turn_filter(Some("t"));
        let turn_sql = format!(
            r#"
            SELECT
                COUNT(*),
                COALESCE(SUM(t.has_edits), 0),
                COALESCE(SUM(t.one_shot), 0),
                COALESCE(SUM(t.retries), 0),
                COALESCE(SUM(CASE WHEN t.category = 'delegation' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN t.category = 'planning' THEN 1 ELSE 0 END), 0)
            FROM usage_turn t
            {}
            "#,
            turn_filter.where_sql()
        );
        let (turns, edit_turns, one_shot_turns, retries, delegation_turns, planning_turns): (
            i64,
            i64,
            i64,
            i64,
            i64,
            i64,
        ) = self.conn.query_row(
            &turn_sql,
            params_from_iter(turn_filter.params().iter()),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;

        let tool_filter = model_filter.tool_filter(Some("tc"));
        let tool_calls = scalar_i64(
            &self.conn,
            &format!(
                "SELECT COUNT(*) FROM usage_tool_call tc{}",
                tool_filter.where_sql()
            ),
            params_from_iter(tool_filter.params().iter()),
        )?;
        let cache_efficiency = ratio(cache_read, input + cache_creation + cache_read);
        Ok(Some(ModelCompareStats {
            model: model.to_string(),
            calls,
            turns,
            edit_turns,
            one_shot_turns,
            retries,
            total_tokens,
            estimated_cost_usd: cost,
            cache_efficiency,
            cost_per_call: ratio_f64(cost, calls),
            cost_per_edit_turn: ratio_f64(cost, edit_turns),
            one_shot_rate: ratio(one_shot_turns, edit_turns),
            retry_rate: ratio(retries, turns),
            avg_tools_per_turn: ratio(tool_calls, turns),
            delegation_rate: ratio(delegation_turns, turns),
            planning_rate: ratio(planning_turns, turns),
            low_sample: calls < 20 || edit_turns < 10,
        }))
    }

    fn category_compare(
        &self,
        filter: &QueryFilter,
        model_a: &str,
        model_b: &str,
    ) -> Result<Vec<CategoryCompareRow>> {
        let mut rows_by_category: BTreeMap<String, (i64, i64, i64, i64)> = BTreeMap::new();
        for (index, model) in [model_a, model_b].into_iter().enumerate() {
            let mut model_filter = filter.clone();
            model_filter.model = Some(model.to_string());
            let turn_filter = model_filter.turn_filter(Some("t"));
            let sql = format!(
                r#"
                SELECT
                    t.category,
                    COALESCE(SUM(t.has_edits), 0) AS edit_turns,
                    COALESCE(SUM(t.one_shot), 0) AS one_shot_turns
                FROM usage_turn t
                {}
                GROUP BY t.category
                HAVING edit_turns > 0
                "#,
                turn_filter.where_sql()
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (category, edit_turns, one_shot_turns) = row?;
                let entry = rows_by_category.entry(category).or_default();
                if index == 0 {
                    entry.0 = edit_turns;
                    entry.1 = one_shot_turns;
                } else {
                    entry.2 = edit_turns;
                    entry.3 = one_shot_turns;
                }
            }
        }
        Ok(rows_by_category
            .into_iter()
            .map(
                |(category, (a_edits, a_one_shot, b_edits, b_one_shot))| CategoryCompareRow {
                    category,
                    model_a_edit_turns: a_edits,
                    model_a_one_shot_rate: ratio(a_one_shot, a_edits),
                    model_b_edit_turns: b_edits,
                    model_b_one_shot_rate: ratio(b_one_shot, b_edits),
                },
            )
            .collect())
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

    /// Loads the flexible Cost Explorer-style aggregate for the requested slice.
    pub fn explorer(&self, query: &ExplorerQuery) -> Result<ExplorerPayload> {
        explorer::load(self, query)
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

    /// Loads the health fields used by the live web shell without returning
    /// thousands of cursor keys that the shell only counts.
    pub fn health_summary(&self) -> Result<HealthSummaryPayload> {
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
            .collect();
        let cursor_count = scalar_i64(&self.conn, "SELECT COUNT(*) FROM source_cursor", [])?;

        Ok(HealthSummaryPayload {
            integrations,
            cursor_count,
            recent_failures,
        })
    }

    /// Builds the top-of-dashboard sync command center payload.
    pub fn sync_command_center(&self, filter: &QueryFilter) -> Result<SyncCommandCenterPayload> {
        let diagnostics = self.diagnostics()?;
        self.sync_command_center_with_diagnostics(filter, &diagnostics)
    }

    fn sync_command_center_with_diagnostics(
        &self,
        filter: &QueryFilter,
        diagnostics: &DiagnosticsPayload,
    ) -> Result<SyncCommandCenterPayload> {
        let statuses = load_sync_statuses_with_conn(&self.conn, filter)?;
        let recent_runs = self.store.run_log().recent_runs_with_conn(&self.conn, 10)?;
        let current_lock = Store::current_worker_lock_with_conn(&self.conn)?;
        let recent_failures = recent_runs
            .iter()
            .filter(|run| matches!(run.command.as_str(), "sync" | "sync --rebuild" | "hook-run"))
            .filter(|run| RunRecord::counts_as_failure(run))
            .count();
        let selected_source = filter.source.map(|source| source.as_str().to_string());
        let risk_sources = diagnostics
            .by_source
            .iter()
            .filter(|source| {
                selected_source
                    .as_deref()
                    .is_none_or(|selected| source.source == selected)
            })
            .filter(|source| source.lossy_rebuild_risk)
            .map(|source| source.source.clone())
            .collect::<Vec<_>>();
        let risk_set = risk_sources.iter().cloned().collect::<BTreeSet<_>>();
        let inserted_total = statuses.iter().map(|row| row.events_inserted).sum::<i64>();
        let seen_total = statuses.iter().map(|row| row.events_seen).sum::<i64>();
        let stored_total = statuses.iter().map(|row| row.stored_events).sum::<i64>();
        let ready_total = statuses
            .iter()
            .filter(|row| row.last_error.is_none() && row.stored_events > 0)
            .count() as i64;
        let sources_total = statuses.len() as i64;
        let max_stored = statuses
            .iter()
            .map(|row| row.stored_events)
            .max()
            .unwrap_or_default()
            .max(1);
        let last_run = recent_runs
            .iter()
            .find(|run| matches!(run.command.as_str(), "sync" | "sync --rebuild" | "hook-run"))
            .map(|run| SyncLastRunPayload {
                status: run.status.clone(),
                command: run.command.clone(),
                started_at: run.started_at.clone(),
                finished_at: run.finished_at.clone(),
                error_key: RunRecord::counts_as_failure(run)
                    .then(|| "syncCenter.reason.lastRunFailed".to_string()),
            });
        let worker_lock = if current_lock.is_some() {
            "busy"
        } else {
            "available"
        }
        .to_string();
        let worker_lock_holder = current_lock.as_ref().map(|lock| lock.holder_identity());
        let lossy_rebuild_risk = !risk_sources.is_empty();
        let tone = if worker_lock == "busy" || recent_failures > 0 || lossy_rebuild_risk {
            "warn"
        } else {
            "good"
        };
        let headline_key = if worker_lock == "busy" {
            "syncCenter.headline.busy"
        } else if recent_failures > 0 {
            "syncCenter.headline.failed"
        } else if lossy_rebuild_risk {
            "syncCenter.headline.rebuildRisk"
        } else {
            "syncCenter.headline.ready"
        };
        let reason_key = if lossy_rebuild_risk {
            "syncCenter.reason.rebuildRisk"
        } else if statuses.is_empty() {
            "syncCenter.reason.empty"
        } else {
            "syncCenter.reason.ready"
        };

        Ok(SyncCommandCenterPayload {
            mode: "live".to_string(),
            tone: tone.to_string(),
            headline_key: headline_key.to_string(),
            reason_key: reason_key.to_string(),
            generated_at: now_utc(),
            current_job: None,
            last_run,
            safety: SyncSafetyPayload {
                ordinary_sync_safe: worker_lock != "busy",
                worker_lock: worker_lock.clone(),
                worker_lock_holder,
                lossy_rebuild_risk,
                risk_sources,
                recent_failures,
            },
            metrics: SyncMetricsPayload {
                events_seen: seen_total,
                inserted_delta: inserted_total,
                stored_events: stored_total,
                sources_ready: ready_total,
                sources_total,
            },
            sources: statuses
                .into_iter()
                .map(|row| {
                    let source_risk = risk_set.contains(&row.source);
                    let status = if row.last_error.is_some() {
                        "error"
                    } else if source_risk {
                        "rebuild_risk"
                    } else if row.stored_events > 0 || row.events_seen > 0 {
                        "ok"
                    } else {
                        "idle"
                    };
                    let tone = match status {
                        "error" | "rebuild_risk" => "warn",
                        "ok" => "good",
                        _ => "neutral",
                    };
                    SyncSourcePayload {
                        source: row.source,
                        status: status.to_string(),
                        tone: tone.to_string(),
                        files_processed: row.files_processed,
                        changed_files: row.changed_files,
                        skipped_files: (row.files_processed - row.changed_files).max(0),
                        events_seen: row.events_seen,
                        events_inserted: row.events_inserted,
                        stored_events: row.stored_events,
                        updated_at: Some(row.updated_at),
                        share: (row.stored_events as f64 / max_stored as f64).clamp(0.0, 1.0),
                        error_key: row
                            .last_error
                            .is_some()
                            .then(|| "syncCenter.reason.sourceError".to_string()),
                        lossy_rebuild_risk: source_risk,
                    }
                })
                .collect(),
            actions: vec![SyncActionPayload {
                id: "sync".to_string(),
                label_key: "syncCenter.action.sync".to_string(),
                primary: true,
                disabled: worker_lock == "busy",
                reason_key: if worker_lock == "busy" {
                    Some("syncCenter.action.busy".to_string())
                } else {
                    None
                },
            }],
        })
    }

    /// Builds the full dashboard snapshot used by static HTML export.
    ///
    /// The snapshot still embeds the legacy four-window trends (`day`/`week`/
    /// `month`/`all`) for backwards-compat HTML export. It intentionally uses
    /// the legacy scalar trend shape because `/api/trends?window=` still
    /// exposes that contract.
    pub fn snapshot(&self, filter: &QueryFilter) -> Result<DashboardSnapshot> {
        let core = self.core_snapshot(filter)?;
        Ok(DashboardSnapshot {
            overview: core.overview,
            sync_command_center: core.sync_command_center,
            day_trends: core.day_trends,
            week_trends: core.week_trends,
            month_trends: core.month_trends,
            all_trends: core.all_trends,
            models: core.models,
            sources: core.sources,
            projects: core.projects,
            costs: core.costs,
            activity: self.activity_breakdown(filter)?,
            tools: self.tool_breakdown(filter)?,
            optimize: self.optimize(filter)?,
            compare: self.model_compare(filter, None, None)?,
            explorer: self.explorer(&ExplorerQuery {
                filter: filter.clone(),
                ..Default::default()
            })?,
            health: core.health,
            diagnostics: core.diagnostics,
        })
    }

    /// Builds the core dashboard sections without behavior analytics.
    ///
    /// Web handlers use this to return the first screen even when
    /// Activity/Tools/Optimize/Compare time out or fail.
    pub fn core_snapshot(&self, filter: &QueryFilter) -> Result<DashboardCoreSnapshot> {
        let diagnostics = self.diagnostics()?;
        self.core_snapshot_with_diagnostics(filter, &diagnostics)
    }

    /// Builds the core sections reusing an already-computed diagnostics
    /// payload.
    ///
    /// The web layer caches `Dashboard::diagnostics()` at the request
    /// boundary and injects the cached value here; `Dashboard::diagnostics`
    /// itself stays a cold read and `home_overview` is untouched.
    pub fn core_snapshot_with_diagnostics(
        &self,
        filter: &QueryFilter,
        diagnostics: &DiagnosticsPayload,
    ) -> Result<DashboardCoreSnapshot> {
        Ok(DashboardCoreSnapshot {
            overview: self.overview(filter)?,
            sync_command_center: self.sync_command_center_with_diagnostics(filter, diagnostics)?,
            day_trends: self.trends("day", filter)?,
            week_trends: self.trends("week", filter)?,
            month_trends: self.trends("month", filter)?,
            all_trends: self.trends("all", filter)?,
            models: self.model_breakdown(filter)?,
            sources: self.source_breakdown(filter)?,
            projects: self.project_breakdown(filter)?,
            costs: self.cost_breakdown(filter)?,
            health: self.health()?,
            diagnostics: diagnostics.clone(),
        })
    }

    /// Builds the range-dependent live projection without legacy trend windows
    /// or full cursor detail.
    pub fn interactive_snapshot(
        &self,
        filter: &QueryFilter,
        window: &str,
    ) -> Result<DashboardInteractiveSnapshot> {
        let diagnostics = self.diagnostics()?;
        self.interactive_snapshot_with_diagnostics(filter, window, &diagnostics)
    }

    /// Builds the interactive projection reusing an already-computed
    /// diagnostics payload. See [`Dashboard::core_snapshot_with_diagnostics`].
    pub fn interactive_snapshot_with_diagnostics(
        &self,
        filter: &QueryFilter,
        window: &str,
        diagnostics: &DiagnosticsPayload,
    ) -> Result<DashboardInteractiveSnapshot> {
        Ok(DashboardInteractiveSnapshot {
            overview: self.overview(filter)?,
            sync_command_center: self.sync_command_center_with_diagnostics(filter, diagnostics)?,
            trends: self.trends(window, filter)?,
            models: self.model_breakdown(filter)?,
            sources: self.source_breakdown(filter)?,
            projects: self.project_breakdown(filter)?,
            costs: self.cost_breakdown(filter)?,
            health: self.health_summary()?,
            diagnostics: diagnostics.clone(),
        })
    }
}

fn context_pressure_event_filter(filter: &QueryFilter) -> filter::SqlFilter {
    let mut event_filter = filter.event_filter(None);
    if filter.source.is_none() && (filter.since.is_some() || filter.until.is_some()) {
        let sources = registered_source_descriptors();
        event_filter.push_raw(format!(
            "source IN ({})",
            std::iter::repeat_n("?", sources.len())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for descriptor in sources {
            event_filter.push_value(rusqlite::types::Value::Text(
                descriptor.stable_id.to_string(),
            ));
        }
    }
    event_filter
}

fn behavior_support(
    conn: &Connection,
    table: &str,
    filter: crate::query::filter::SqlFilter,
) -> Result<BehaviorSupport> {
    let count = scalar_i64(
        conn,
        &format!("SELECT COUNT(*) FROM {table}{}", filter.where_sql()),
        params_from_iter(filter.params().iter()),
    )?;
    Ok(if count > 0 {
        BehaviorSupport {
            supported: true,
            level: "normalized".to_string(),
            reason: None,
        }
    } else {
        BehaviorSupport {
            supported: false,
            level: "no_data".to_string(),
            reason: Some(
                "No normalized behavior facts match this filter; run sync with a parser that emits behavior facts."
                    .to_string(),
            ),
        }
    })
}

fn compare_model_candidates(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<Vec<CompareModelCandidate>> {
    let bucket_filter = filter.bucket_filter(Some("b"));
    let sql = format!(
        r#"
        SELECT
            b.model,
            COALESCE(SUM(b.event_count), 0) AS calls,
            COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(b.cost_with_cache_usd), 0.0) AS estimated_cost_usd
        FROM usage_bucket_30m b
        {}
        GROUP BY b.model
        ORDER BY estimated_cost_usd DESC, total_tokens DESC, calls DESC, b.model ASC
        LIMIT 25
        "#,
        bucket_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(bucket_filter.params().iter()), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
            row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
            row.get::<_, Option<f64>>(3)?.unwrap_or_default(),
        ))
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let (model, calls, total_tokens, estimated_cost_usd) = row?;
        candidates.push(CompareModelCandidate {
            model,
            calls,
            turns: 0,
            edit_turns: 0,
            total_tokens,
            estimated_cost_usd,
            low_sample: true,
        });
    }

    // One grouped turn query covers every candidate instead of one query per
    // model. A candidate without matching turns keeps the (0, 0) defaults,
    // which matches the legacy per-model `COUNT(*)` result for empty sets.
    if !candidates.is_empty() {
        let turn_filter = filter.turn_filter(Some("t"));
        let turn_sql = format!(
            "SELECT t.primary_model, COUNT(*), COALESCE(SUM(t.has_edits), 0) FROM usage_turn t{} GROUP BY t.primary_model",
            turn_filter.where_sql()
        );
        let mut turn_stmt = conn.prepare(&turn_sql)?;
        let turn_rows =
            turn_stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
        let mut turn_stats: std::collections::HashMap<String, (i64, i64)> =
            std::collections::HashMap::new();
        for row in turn_rows {
            let (model, turns, edit_turns) = row?;
            turn_stats.insert(model, (turns, edit_turns));
        }
        for candidate in &mut candidates {
            let (turns, edit_turns) = turn_stats.get(&candidate.model).copied().unwrap_or((0, 0));
            candidate.turns = turns;
            candidate.edit_turns = edit_turns;
            candidate.low_sample = candidate.calls < 20 || candidate.edit_turns < 10;
        }
    }
    Ok(candidates)
}

fn severity_rank(severity: &str) -> i64 {
    match severity {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

fn health_grade(score: i64) -> &'static str {
    match score {
        90..=100 => "A",
        80..=89 => "B",
        70..=79 => "C",
        60..=69 => "D",
        _ => "F",
    }
}

fn safe_short(value: &str, max_chars: usize) -> String {
    let mut out = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        out.push('…');
    }
    out
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator <= 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn ratio_f64(numerator: f64, denominator: i64) -> f64 {
    if denominator <= 0 {
        0.0
    } else {
        numerator / denominator as f64
    }
}

fn compare_metrics(left: &ModelCompareStats, right: &ModelCompareStats) -> Vec<CompareMetric> {
    vec![
        CompareMetric {
            id: "one_shot_rate".to_string(),
            label: "One-shot rate".to_string(),
            model_a_value: left.one_shot_rate,
            model_b_value: right.one_shot_rate,
            higher_is_better: true,
        },
        CompareMetric {
            id: "retry_rate".to_string(),
            label: "Retry rate".to_string(),
            model_a_value: left.retry_rate,
            model_b_value: right.retry_rate,
            higher_is_better: false,
        },
        CompareMetric {
            id: "cost_per_call".to_string(),
            label: "Cost / call".to_string(),
            model_a_value: left.cost_per_call,
            model_b_value: right.cost_per_call,
            higher_is_better: false,
        },
        CompareMetric {
            id: "cost_per_edit_turn".to_string(),
            label: "Cost / edit".to_string(),
            model_a_value: left.cost_per_edit_turn,
            model_b_value: right.cost_per_edit_turn,
            higher_is_better: false,
        },
        CompareMetric {
            id: "cache_efficiency".to_string(),
            label: "Cache efficiency".to_string(),
            model_a_value: left.cache_efficiency,
            model_b_value: right.cache_efficiency,
            higher_is_better: true,
        },
    ]
}

fn working_style_metrics(
    left: &ModelCompareStats,
    right: &ModelCompareStats,
) -> Vec<CompareMetric> {
    vec![
        CompareMetric {
            id: "delegation_rate".to_string(),
            label: "Delegation".to_string(),
            model_a_value: left.delegation_rate,
            model_b_value: right.delegation_rate,
            higher_is_better: true,
        },
        CompareMetric {
            id: "planning_rate".to_string(),
            label: "Planning".to_string(),
            model_a_value: left.planning_rate,
            model_b_value: right.planning_rate,
            higher_is_better: true,
        },
        CompareMetric {
            id: "tools_per_turn".to_string(),
            label: "Tools / turn".to_string(),
            model_a_value: left.avg_tools_per_turn,
            model_b_value: right.avg_tools_per_turn,
            higher_is_better: true,
        },
    ]
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
            COALESCE(SUM(cache_creation_tokens), 0),
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
        cache_creation_tokens: row.get(1)?,
        cache_read_tokens: row.get(2)?,
        output_tokens: row.get(3)?,
        reasoning_output_tokens: row.get(4)?,
        total_tokens: row.get(5)?,
    })
}

#[derive(Debug)]
struct SyncStatusRow {
    source: String,
    files_processed: i64,
    changed_files: i64,
    events_seen: i64,
    events_inserted: i64,
    stored_events: i64,
    updated_at: String,
    last_error: Option<String>,
}

fn load_sync_statuses_with_conn(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<Vec<SyncStatusRow>> {
    let source = filter.source.map(|source| source.as_str().to_string());
    let mut stmt = conn.prepare(
        r#"
        SELECT source, files_processed, changed_files, events_seen, events_inserted,
               stored_events, updated_at
        FROM source_sync_status
        WHERE (?1 IS NULL OR source = ?1)
        ORDER BY stored_events DESC, source ASC
        "#,
    )?;
    let rows = stmt.query_map([source], |row| {
        Ok(SyncStatusRow {
            source: row.get(0)?,
            files_processed: row.get(1)?,
            changed_files: row.get(2)?,
            events_seen: row.get(3)?,
            events_inserted: row.get(4)?,
            stored_events: row.get(5)?,
            updated_at: row.get(6)?,
            last_error: None,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Loads `SourceDiagnostics` rows by joining the per-source state counts in
/// `source_file` with the recent/history completion timestamps in
/// `source_sync_status`. Rows show up for any source that appears in either
/// table, sorted by source identifier.
#[cfg(test)]
static DIAGNOSTICS_STAT_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_diagnostics_stat_counter() {
    DIAGNOSTICS_STAT_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn diagnostics_stat_calls() -> usize {
    DIAGNOSTICS_STAT_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

fn load_source_diagnostics(conn: &Connection) -> Result<Vec<SourceDiagnostics>> {
    // Pre-load all source_file paths to avoid N+1 queries in the main loop
    let mut file_paths_by_source: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT source, file_path FROM source_file")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (source, path) = row?;
            file_paths_by_source.entry(source).or_default().push(path);
        }
    }

    let missing_file_counts: std::collections::HashMap<String, u64> = file_paths_by_source
        .iter()
        .filter_map(|(source, paths)| {
            let missing = paths
                .iter()
                .filter(|path| {
                    #[cfg(test)]
                    DIAGNOSTICS_STAT_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    !std::path::Path::new(path).exists()
                })
                .count() as u64;
            (missing > 0).then_some((source.clone(), missing))
        })
        .collect();

    // `event_count` is maintained transactionally with usage_event writes, so
    // diagnostics can avoid scanning the full fact table on every dashboard load.
    // It is only needed for sources with missing files; querying all buckets when
    // every source file is live makes the common dashboard path needlessly scan
    // the entire aggregate projection.
    let mut event_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    if !missing_file_counts.is_empty() {
        let placeholders = std::iter::repeat_n("?", missing_file_counts.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT source, COALESCE(SUM(event_count), 0) FROM usage_bucket_30m WHERE source IN ({placeholders}) GROUP BY source"
        );
        let sources = missing_file_counts.keys().collect::<Vec<_>>();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(sources), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.max(0) as u64,
            ))
        })?;
        for row in rows {
            let (source, count) = row?;
            event_counts.insert(source, count);
        }
    }

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
        let missing_file_count = missing_file_counts.get(&source).copied().unwrap_or(0);
        let total_events = if missing_file_count > 0 {
            *event_counts.get(&source).unwrap_or(&0)
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
pub(crate) fn activity_query_plan_for_test(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<Vec<String>> {
    let turn_filter = filter.turn_filter(Some("t"));
    let sql = format!(
        r#"
        EXPLAIN QUERY PLAN
        SELECT
            t.category,
            COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS estimated_cost_usd
        FROM usage_turn t
        LEFT JOIN usage_event e
            ON e.event_key = substr(t.turn_key, 6)
        {}
        GROUP BY t.category
        "#,
        turn_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
        row.get::<_, String>(3)
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
pub(crate) fn session_outlier_query_plan_for_test(
    conn: &Connection,
    filter: &QueryFilter,
) -> Result<Vec<String>> {
    let turn_filter = filter.turn_filter(Some("t"));
    let sql = format!(
        r#"
        EXPLAIN QUERY PLAN
        SELECT
            t.session_id,
            COALESCE(SUM(e.cost_with_cache_usd), 0.0) AS cost
        FROM usage_turn t
        LEFT JOIN usage_event e ON e.event_key = substr(t.turn_key, 6)
        {}
        GROUP BY t.session_id
        "#,
        turn_filter.where_sql()
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(turn_filter.params().iter()), |row| {
        row.get::<_, String>(3)
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use chrono::{FixedOffset, NaiveDate};

    use super::{
        Dashboard, QueryFilter, ReportTimezone, context_pressure_event_filter, home_overview,
    };
    use crate::{
        models::SourceKind,
        store::Store,
        testing::{Fixture, SeedEvent},
    };

    const EPSILON: f64 = 1e-9;

    #[test]
    fn diagnostics_counts_protected_events_from_aggregate_projection() -> Result<()> {
        let fixture = Fixture::new()?;
        for (event_key, tokens) in [("codex:diagnostics:1", 10), ("codex:diagnostics:2", 20)] {
            fixture.seed_event(SeedEvent {
                event_key,
                input_tokens: tokens,
                total_tokens: tokens,
                ..Default::default()
            })?;
        }
        let conn = fixture.store().open_connection()?;
        conn.execute(
            "INSERT INTO source_file(source, file_path, state, last_state_change_at) VALUES ('codex', ?1, 'live', '2026-07-11T00:00:00Z')",
            [fixture.paths().root_dir.join("missing-session.jsonl").display().to_string()],
        )?;
        // The aggregate projection remains populated while the fact rows are
        // removed, making this a direct regression for the diagnostics route.
        conn.execute("DELETE FROM usage_event", [])?;
        drop(conn);

        let diagnostics = Dashboard::open(fixture.store())?.diagnostics()?;
        let codex = diagnostics
            .by_source
            .iter()
            .find(|row| row.source == "codex")
            .expect("codex diagnostics");
        assert_eq!(codex.missing_file_count, 1);
        assert_eq!(codex.protected_event_count, 2);
        assert!(codex.lossy_rebuild_risk);
        Ok(())
    }

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
    fn source_breakdown_preserves_filtered_latest_event_time() -> Result<()> {
        let fixture = Fixture::new()?;
        for event in [
            SeedEvent {
                event_key: "codex:source-breakdown:early",
                event_at: "2026-05-01T01:00:00Z",
                input_tokens: 10,
                total_tokens: 10,
                project_hash: "project-a",
                ..Default::default()
            },
            SeedEvent {
                event_key: "codex:source-breakdown:latest-matching",
                event_at: "2026-05-02T23:00:00Z",
                input_tokens: 20,
                total_tokens: 20,
                project_hash: "project-a",
                ..Default::default()
            },
            SeedEvent {
                event_key: "codex:source-breakdown:other-model",
                model: "gpt-other",
                event_at: "2026-05-03T00:00:00Z",
                input_tokens: 30,
                total_tokens: 30,
                project_hash: "project-a",
                ..Default::default()
            },
            SeedEvent {
                event_key: "codex:source-breakdown:other-project",
                event_at: "2026-05-04T00:00:00Z",
                input_tokens: 40,
                total_tokens: 40,
                project_hash: "project-b",
                ..Default::default()
            },
            SeedEvent {
                event_key: "claude:source-breakdown:latest",
                source: "claude",
                model: "claude-sonnet-4-5",
                event_at: "2026-05-05T00:00:00Z",
                input_tokens: 50,
                total_tokens: 50,
                project_hash: "project-a",
                ..Default::default()
            },
        ] {
            fixture.seed_event(event)?;
        }

        let dashboard = Dashboard::open(fixture.store())?;
        let filtered = dashboard.source_breakdown(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
            project_hash: Some("project-a".to_string()),
            timezone: ReportTimezone::Utc,
        })?;

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].source, "codex");
        assert_eq!(filtered[0].total_tokens, 30);
        assert_eq!(filtered[0].event_count, 2);
        assert_eq!(
            filtered[0].last_event_at.as_deref(),
            Some("2026-05-02T23:00:00Z")
        );

        let all = dashboard.source_breakdown(&QueryFilter::default())?;
        assert_eq!(
            all.iter()
                .find(|row| row.source == "codex")
                .and_then(|row| row.last_event_at.as_deref()),
            Some("2026-05-04T00:00:00Z")
        );
        assert_eq!(
            all.iter()
                .find(|row| row.source == "claude")
                .and_then(|row| row.last_event_at.as_deref()),
            Some("2026-05-05T00:00:00Z")
        );
        Ok(())
    }

    #[test]
    fn context_pressure_ratios_and_unpriced_split() -> Result<()> {
        use crate::testing::SeedEvent;

        let fixture = Fixture::new()?;
        // Priced model (codex gpt-5, window 400_000): peak prompt 200_000 -> 50%.
        fixture.seed_event(SeedEvent {
            event_key: "codex:ctx:1",
            model: "gpt-5",
            input_tokens: 150_000,
            cache_read_tokens: 50_000,
            total_tokens: 200_000,
            ..SeedEvent::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:ctx:2",
            model: "gpt-5",
            input_tokens: 40_000,
            total_tokens: 40_000,
            ..SeedEvent::default()
        })?;
        // Unknown-window model is excluded from ratios but counted as unpriced.
        fixture.seed_event(SeedEvent {
            event_key: "codex:ctx:3",
            model: "mystery-model",
            input_tokens: 999_999,
            total_tokens: 999_999,
            ..SeedEvent::default()
        })?;

        let dashboard = Dashboard::open(fixture.store())?;
        let pressure = dashboard.context_pressure(&Default::default())?;

        assert!((pressure.peak_percent - 0.5).abs() < 1e-9);
        // avg = (200_000 + 40_000) / 400_000 / 2 priced events = 0.30
        assert!((pressure.avg_percent - 0.30).abs() < 1e-9);
        assert_eq!(pressure.priced_events, 2);
        assert_eq!(pressure.unpriced_events, 1);
        assert_eq!(pressure.peak_model.as_deref(), Some("codex:gpt-5"));
        Ok(())
    }

    #[test]
    fn bounded_context_pressure_uses_source_time_ranges_without_changing_totals() -> Result<()> {
        let fixture = Fixture::new()?;
        for event in [
            SeedEvent {
                event_key: "codex:bounded:1",
                source: "codex",
                model: "gpt-5",
                event_at: "2026-05-08T01:00:00Z",
                input_tokens: 200_000,
                total_tokens: 200_000,
                ..Default::default()
            },
            SeedEvent {
                event_key: "claude:bounded:1",
                source: "claude",
                model: "claude-fable-5",
                event_at: "2026-05-08T02:00:00Z",
                input_tokens: 500_000,
                total_tokens: 500_000,
                ..Default::default()
            },
            SeedEvent {
                event_key: "codex:outside",
                source: "codex",
                model: "gpt-5",
                event_at: "2026-05-07T23:59:59Z",
                input_tokens: 400_000,
                total_tokens: 400_000,
                ..Default::default()
            },
        ] {
            fixture.seed_event(event)?;
        }
        let filter = QueryFilter {
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 8).unwrap()),
            timezone: ReportTimezone::Utc,
            ..Default::default()
        };
        let dashboard = Dashboard::open(fixture.store())?;
        let combined = dashboard.context_pressure(&filter)?;
        let codex = dashboard.context_pressure(&QueryFilter {
            source: Some(SourceKind::Codex),
            ..filter.clone()
        })?;
        let claude = dashboard.context_pressure(&QueryFilter {
            source: Some(SourceKind::Claude),
            ..filter.clone()
        })?;

        assert_eq!(
            combined.priced_events,
            codex.priced_events + claude.priced_events
        );
        assert_eq!(
            combined.unpriced_events,
            codex.unpriced_events + claude.unpriced_events
        );
        let expected_avg = (codex.avg_percent * codex.priced_events as f64
            + claude.avg_percent * claude.priced_events as f64)
            / combined.priced_events as f64;
        assert!((combined.avg_percent - expected_avg).abs() < EPSILON);
        assert_eq!(
            combined.peak_percent,
            codex.peak_percent.max(claude.peak_percent)
        );

        let event_filter = context_pressure_event_filter(&filter);
        let sql = format!(
            "EXPLAIN QUERY PLAN SELECT source, model, MAX(input_tokens + cache_read_tokens + cache_creation_tokens), SUM(input_tokens + cache_read_tokens + cache_creation_tokens), COUNT(*) FROM usage_event {} GROUP BY source, model",
            event_filter.where_sql()
        );
        let conn = fixture.store().open_connection()?;
        let mut stmt = conn.prepare(&sql)?;
        let plan = stmt
            .query_map(
                rusqlite::params_from_iter(event_filter.params().iter()),
                |row| row.get::<_, String>(3),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        assert!(
            plan.iter()
                .any(|detail| detail.contains("idx_usage_event_source_event_at")),
            "{plan:?}"
        );
        Ok(())
    }

    #[test]
    fn context_pressure_knows_claude_fable_and_mythos_windows() -> Result<()> {
        use crate::testing::SeedEvent;

        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "claude:ctx:fable",
            source: "claude",
            model: "claude-fable-5",
            input_tokens: 450_000,
            cache_read_tokens: 50_000,
            total_tokens: 500_000,
            ..SeedEvent::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "claude:ctx:mythos",
            source: "claude",
            model: "claude-mythos-5",
            input_tokens: 250_000,
            total_tokens: 250_000,
            ..SeedEvent::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "claude:ctx:unknown",
            source: "claude",
            model: "claude-mythos-preview",
            input_tokens: 1_000_000,
            total_tokens: 1_000_000,
            ..SeedEvent::default()
        })?;

        let dashboard = Dashboard::open(fixture.store())?;
        let pressure = dashboard.context_pressure(&Default::default())?;

        assert!((pressure.peak_percent - 0.5).abs() < 1e-9);
        assert!((pressure.avg_percent - 0.375).abs() < 1e-9);
        assert_eq!(pressure.priced_events, 2);
        assert_eq!(pressure.unpriced_events, 1);
        assert_eq!(
            pressure.peak_model.as_deref(),
            Some("claude:claude-fable-5")
        );
        Ok(())
    }

    #[test]
    fn context_pressure_empty_is_zero() -> Result<()> {
        let fixture = Fixture::new()?;
        let dashboard = Dashboard::open(fixture.store())?;
        let pressure = dashboard.context_pressure(&Default::default())?;
        assert_eq!(pressure.priced_events, 0);
        assert_eq!(pressure.unpriced_events, 0);
        assert_eq!(pressure.peak_percent, 0.0);
        assert_eq!(pressure.avg_percent, 0.0);
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
        assert_eq!(model.pricing_source.as_deref(), Some("static-v2"));
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

    #[test]
    fn behavior_queries_return_activity_and_tool_breakdowns() -> Result<()> {
        let fixture = Fixture::new()?;
        let conn = fixture.store().open_connection()?;

        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:behavior:multi-tool",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 120,
            output_tokens: 60,
            total_tokens: 180,
            cost_with_cache_usd: 1.00,
            cost_without_cache_usd: 1.00,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-behavior"),
            source_path_hash: Some("path-behavior"),
            ..Default::default()
        })?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:behavior:non-tool",
            event_at: "2026-05-02T01:00:00Z",
            hour_start: Some("2026-05-02T01:00:00Z"),
            input_tokens: 80,
            output_tokens: 20,
            total_tokens: 100,
            cost_with_cache_usd: 0.25,
            cost_without_cache_usd: 0.25,
            pricing_status: "static",
            pricing_source: Some("static-v1"),
            session_id: Some("session-behavior"),
            source_path_hash: Some("path-behavior"),
            ..Default::default()
        })?;
        conn.execute(
            r#"
            INSERT INTO usage_turn(
                turn_key, source, session_id, source_path_hash, project_hash,
                primary_model, started_at, category, has_edits, retries,
                one_shot, call_count, input_tokens, cache_read_tokens,
                cache_creation_tokens, output_tokens, reasoning_output_tokens,
                total_tokens, created_at
            ) VALUES ('turn:codex:behavior:multi-tool', 'codex', 'session-behavior',
                'path-behavior', 'project-test', 'gpt-5', '2026-05-01T00:00:00Z',
                'coding', 1, 0, 1, 1, 100, 0, 0, 50, 0, 150, '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_turn(
                turn_key, source, session_id, source_path_hash, project_hash,
                primary_model, started_at, category, has_edits, retries,
                one_shot, call_count, input_tokens, cache_read_tokens,
                cache_creation_tokens, output_tokens, reasoning_output_tokens,
                total_tokens, created_at
            ) VALUES ('turn:codex:behavior:non-tool', 'codex', 'session-behavior',
                'path-behavior', 'project-test', 'gpt-5', '2026-05-02T01:00:00Z',
                'coding', 0, 0, 0, 1, 80, 0, 0, 20, 0, 100, '2026-05-02T01:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES ('tool:codex:behavior:multi-tool:edit',
                'turn:codex:behavior:multi-tool', 'codex:behavior:multi-tool', 'codex',
                'session-behavior', 'path-behavior', 'project-test', 'gpt-5',
                '2026-05-01T00:00:00Z', 'Edit', 'edit',
                NULL, NULL, 'fp-edit', 'Edit src/lib.rs', '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES ('tool:codex:behavior:multi-tool:read',
                'turn:codex:behavior:multi-tool', 'codex:behavior:multi-tool', 'codex',
                'session-behavior', 'path-behavior', 'project-test', 'gpt-5',
                '2026-05-01T00:00:00Z', 'Read', 'read',
                NULL, NULL, 'fp-read', 'Read src/lib.rs', '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        drop(conn);

        let dashboard = Dashboard::open(fixture.store())?;
        let activity = dashboard.activity_breakdown(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            ..Default::default()
        })?;
        assert!(activity.support.supported);
        assert_eq!(activity.breakdown.len(), 1);
        assert_eq!(activity.breakdown[0].category, "coding");
        assert_eq!(activity.breakdown[0].turns, 2);
        assert_eq!(activity.breakdown[0].one_shot_rate, 1.0);
        assert_eq!(activity.breakdown[0].estimated_cost_usd, 1.25);

        let tools = dashboard.tool_breakdown(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            ..Default::default()
        })?;
        assert!(tools.support.supported);
        assert_eq!(tools.breakdown.len(), 3);
        let total_cost: f64 = tools
            .breakdown
            .iter()
            .map(|row| row.estimated_cost_usd)
            .sum();
        assert!((total_cost - 1.25).abs() < f64::EPSILON);

        let edit = tools
            .breakdown
            .iter()
            .find(|row| row.tool_name == "Edit")
            .expect("edit row");
        assert_eq!(edit.tool_kind, "edit");
        assert_eq!(edit.calls, 1);
        assert_eq!(edit.turn_count, 1);
        assert_eq!(edit.session_count, 1);
        assert_eq!(edit.call_share, 0.5);
        assert_eq!(edit.estimated_cost_usd, 0.5);

        let read = tools
            .breakdown
            .iter()
            .find(|row| row.tool_name == "Read")
            .expect("read row");
        assert_eq!(read.tool_kind, "read");
        assert_eq!(read.calls, 1);
        assert_eq!(read.turn_count, 1);
        assert_eq!(read.session_count, 1);
        assert_eq!(read.call_share, 0.5);
        assert_eq!(read.estimated_cost_usd, 0.5);

        let non_tool = tools
            .breakdown
            .iter()
            .find(|row| row.tool_name == "(non-tool)")
            .expect("non-tool row");
        assert_eq!(non_tool.tool_kind, "(non-tool)");
        assert_eq!(non_tool.calls, 0);
        assert_eq!(non_tool.turn_count, 1);
        assert_eq!(non_tool.session_count, 1);
        assert_eq!(non_tool.call_share, 0.0);
        assert_eq!(non_tool.estimated_cost_usd, 0.25);

        let day_one = dashboard.tool_breakdown(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
            timezone: ReportTimezone::Utc,
            ..Default::default()
        })?;
        assert_eq!(day_one.breakdown.len(), 2);
        let day_one_cost: f64 = day_one
            .breakdown
            .iter()
            .map(|row| row.estimated_cost_usd)
            .sum();
        assert!((day_one_cost - 1.0).abs() < f64::EPSILON);
        assert!(
            day_one
                .breakdown
                .iter()
                .all(|row| row.tool_name != "(non-tool)")
        );

        let day_two = dashboard.tool_breakdown(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2026, 5, 2).unwrap()),
            timezone: ReportTimezone::Utc,
            ..Default::default()
        })?;
        assert_eq!(day_two.breakdown.len(), 1);
        let day_two_cost: f64 = day_two
            .breakdown
            .iter()
            .map(|row| row.estimated_cost_usd)
            .sum();
        assert!((day_two_cost - 0.25).abs() < f64::EPSILON);
        assert_eq!(day_two.breakdown[0].tool_name, "(non-tool)");
        Ok(())
    }

    #[test]
    fn behavior_event_join_query_plan_uses_event_key_index() -> Result<()> {
        let fixture = Fixture::new()?;
        let conn = fixture.store().open_connection()?;
        for idx in 0..2_000 {
            fixture.seed_event(crate::testing::SeedEvent {
                event_key: &format!("codex:plan:{idx}"),
                source: "codex",
                model: "gpt-5",
                event_at: "2026-05-01T00:00:00Z",
                hour_start: Some("2026-05-01T00:00:00Z"),
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                cost_with_cache_usd: 0.01,
                cost_without_cache_usd: 0.01,
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                source_path_hash: Some("path-plan"),
                session_id: Some("session-plan"),
                ..crate::testing::SeedEvent::default()
            })?;
            conn.execute(
                r#"
                INSERT INTO usage_turn(
                    turn_key, source, session_id, source_path_hash, project_hash,
                    primary_model, started_at, category, has_edits, retries,
                    one_shot, call_count, input_tokens, cache_read_tokens,
                    cache_creation_tokens, output_tokens, reasoning_output_tokens,
                    total_tokens, created_at
                ) VALUES (?1, 'codex', 'session-plan', 'path-plan',
                    'project-test', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                    1, 0, 1, 1, 10, 0, 0, 5, 0, 15, '2026-05-01T00:00:00Z')
                "#,
                [format!("turn:codex:plan:{idx}")],
            )?;
        }

        let filter = QueryFilter {
            source: Some(SourceKind::Codex),
            ..QueryFilter::default()
        };
        for plan in [
            super::activity_query_plan_for_test(&conn, &filter)?,
            super::session_outlier_query_plan_for_test(&conn, &filter)?,
        ] {
            let details = plan.join("\n");
            assert!(
                details.contains("USING INDEX sqlite_autoindex_usage_event_1")
                    || details.contains("USING COVERING INDEX sqlite_autoindex_usage_event_1"),
                "usage_event join must probe event_key index, plan was:\n{details}"
            );
            assert!(
                !details.contains("SCAN e"),
                "usage_event join must not full-scan e, plan was:\n{details}"
            );
        }
        Ok(())
    }

    #[test]
    fn behavior_queries_return_explicit_no_data_support() -> Result<()> {
        let fixture = Fixture::new()?;
        let dashboard = Dashboard::open(fixture.store())?;

        let activity = dashboard.activity_breakdown(&QueryFilter::default())?;
        let tools = dashboard.tool_breakdown(&QueryFilter::default())?;

        assert!(!activity.support.supported);
        assert_eq!(activity.support.level, "no_data");
        assert!(activity.breakdown.is_empty());
        assert!(!tools.support.supported);
        assert_eq!(tools.support.level, "no_data");
        assert!(tools.breakdown.is_empty());
        Ok(())
    }

    #[test]
    fn zombie_report_diffs_installed_against_used() -> Result<()> {
        use super::InventoryRoots;

        let fixture = Fixture::new()?;
        let conn = fixture.store().open_connection()?;
        let seed = |source: &str, kind: &str, name: &str, server: Option<&str>| -> Result<()> {
            conn.execute(
                r#"INSERT INTO usage_tool_call(
                    tool_call_key, source, occurred_at, tool_name, tool_kind, mcp_server, created_at
                ) VALUES (?1, ?2, '2026-05-01T00:00:00Z', ?3, ?4, ?5, '2026-05-01T00:00:00Z')"#,
                rusqlite::params![
                    format!("tc:{source}:{kind}:{name}"),
                    source,
                    name,
                    kind,
                    server
                ],
            )?;
            Ok(())
        };
        // Used set: claude skill alpha, claude mcp context7, opencode skill gamma.
        seed("claude", "skill", "alpha", None)?;
        seed("claude", "mcp", "context7/search", Some("context7"))?;
        seed("opencode", "skill", "gamma", None)?;

        // Installed roots in a temp tree (superset of the used set).
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let write = |rel: &str, body: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        };
        write("claude/skills/alpha/SKILL.md", "x");
        write("claude/skills/beta/SKILL.md", "x");
        write(
            "claude.json",
            r#"{"mcpServers":{"context7":{},"playwright":{}}}"#,
        );
        write("opencode/skills/gamma/SKILL.md", "x");
        write("opencode/skills/delta/SKILL.md", "x");
        write("opencode/opencode.json", r#"{"mcp":{}}"#);
        let roots = InventoryRoots {
            claude_skills: root.join("claude/skills"),
            claude_mcp_config: root.join("claude.json"),
            codex_skills: root.join("codex/skills"),
            codex_mcp_config: root.join("codex/config.toml"),
            opencode_skills: root.join("opencode/skills"),
            opencode_mcp_config: root.join("opencode/opencode.json"),
        };

        let report = Dashboard::open(fixture.store())?.zombie_report(&roots)?;
        let zombies: std::collections::BTreeSet<(String, String, String)> = report
            .zombies
            .iter()
            .map(|item| (item.source.clone(), item.kind.clone(), item.name.clone()))
            .collect();

        // Installed but never called → zombie candidates.
        assert!(zombies.contains(&("claude".into(), "skill".into(), "beta".into())));
        assert!(zombies.contains(&("claude".into(), "mcp".into(), "playwright".into())));
        assert!(zombies.contains(&("opencode".into(), "skill".into(), "delta".into())));
        // Actually-used items are never flagged.
        assert!(!zombies.iter().any(|item| item.2 == "alpha"));
        assert!(!zombies.iter().any(|item| item.2 == "context7"));
        assert!(!zombies.iter().any(|item| item.2 == "gamma"));
        Ok(())
    }

    #[test]
    fn optimize_returns_read_only_findings_from_behavior_facts() -> Result<()> {
        let fixture = Fixture::new()?;
        for index in 0..8 {
            let event_key = format!("codex:optimize:{index}");
            fixture.seed_event(crate::testing::SeedEvent {
                event_key: &event_key,
                source: "codex",
                model: "gpt-5",
                event_at: "2026-05-01T00:00:00Z",
                hour_start: Some("2026-05-01T00:00:00Z"),
                input_tokens: 100,
                output_tokens: 50,
                total_tokens: 150,
                cost_with_cache_usd: 0.10,
                cost_without_cache_usd: 0.10,
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                session_id: Some("session-optimize"),
                source_path_hash: Some("path-optimize"),
                created_at: Some("2026-05-01T00:00:00Z"),
                ..Default::default()
            })?;
        }
        let conn = fixture.store().open_connection()?;
        for index in 0..8 {
            conn.execute(
                r#"
                INSERT INTO usage_turn(
                    turn_key, source, session_id, source_path_hash, project_hash,
                    primary_model, started_at, category, has_edits, retries,
                    one_shot, call_count, input_tokens, cache_read_tokens,
                    cache_creation_tokens, output_tokens, reasoning_output_tokens,
                    total_tokens, created_at
                ) VALUES (?1, 'codex', 'session-optimize', 'path-optimize',
                    'project-test', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                    1, 0, 1, 1, 100, 0, 0, 50, 0, 150, '2026-05-01T00:00:00Z')
                "#,
                [format!("turn:codex:optimize:{index}")],
            )?;
            conn.execute(
                r#"
                INSERT INTO usage_tool_call(
                    tool_call_key, turn_key, event_key, source, session_id,
                    source_path_hash, project_hash, model, occurred_at, tool_name,
                    tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
                ) VALUES (?1, ?2, ?3, 'codex', 'session-optimize', 'path-optimize',
                    'project-test', 'gpt-5', '2026-05-01T00:00:00Z', 'Edit', 'edit',
                    NULL, NULL, ?4, 'Edit src/lib.rs', '2026-05-01T00:00:00Z')
                "#,
                rusqlite::params![
                    format!("tool:edit:{index}"),
                    format!("turn:codex:optimize:{index}"),
                    format!("codex:optimize:{index}"),
                    format!("fp-edit-{index}")
                ],
            )?;
        }
        for index in 0..3 {
            conn.execute(
                r#"
                INSERT INTO usage_tool_call(
                    tool_call_key, turn_key, event_key, source, session_id,
                    source_path_hash, project_hash, model, occurred_at, tool_name,
                    tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
                ) VALUES (?1, 'turn:codex:optimize:0', 'codex:optimize:0',
                    'codex', 'session-optimize', 'path-optimize', 'project-test',
                    'gpt-5', '2026-05-01T00:00:00Z', 'Read', 'read',
                    NULL, NULL, 'fp-node-modules', 'Read node_modules/pkg/index.js',
                    '2026-05-01T00:00:00Z')
                "#,
                [format!("tool:read:{index}")],
            )?;
        }
        drop(conn);

        let optimize = Dashboard::open(fixture.store())?.optimize(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            ..Default::default()
        })?;

        assert!(optimize.support.supported);
        assert!(optimize.score < 100);
        assert!(
            optimize
                .findings
                .iter()
                .any(|finding| finding.id == "low_read_edit_ratio")
        );
        assert!(
            optimize
                .findings
                .iter()
                .any(|finding| finding.id == "duplicate_reads")
        );
        assert!(
            optimize
                .findings
                .iter()
                .any(|finding| finding.id == "junk_reads")
        );
        assert!(optimize.estimated_savings_tokens > 0);
        assert!(optimize.findings.iter().all(|finding| {
            !finding
                .recommendation
                .to_ascii_lowercase()
                .contains("delete")
        }));
        Ok(())
    }

    #[test]
    fn compare_returns_candidates_metrics_and_low_sample_warning() -> Result<()> {
        let fixture = Fixture::new()?;
        for (index, model, category, has_edits, one_shot, retries) in [
            (0, "gpt-5", "coding", 1, 1, 0),
            (1, "gpt-5", "planning", 0, 0, 0),
            (2, "sonnet", "coding", 1, 0, 1),
            (3, "sonnet", "delegation", 1, 1, 0),
        ] {
            let event_key = format!("codex:compare:{index}");
            fixture.seed_event(crate::testing::SeedEvent {
                event_key: &event_key,
                source: "codex",
                model,
                event_at: "2026-05-01T00:00:00Z",
                hour_start: Some("2026-05-01T00:00:00Z"),
                input_tokens: 100 + index * 10,
                cache_read_tokens: 10,
                output_tokens: 50,
                total_tokens: 160 + index * 10,
                cost_with_cache_usd: 0.10 + (index as f64 * 0.01),
                cost_without_cache_usd: 0.10 + (index as f64 * 0.01),
                pricing_status: "static",
                pricing_source: Some("static-v1"),
                session_id: Some("session-compare"),
                source_path_hash: Some("path-compare"),
                created_at: Some("2026-05-01T00:00:00Z"),
                ..Default::default()
            })?;
            let conn = fixture.store().open_connection()?;
            conn.execute(
                r#"
                INSERT INTO usage_turn(
                    turn_key, source, session_id, source_path_hash, project_hash,
                    primary_model, started_at, category, has_edits, retries,
                    one_shot, call_count, input_tokens, cache_read_tokens,
                    cache_creation_tokens, output_tokens, reasoning_output_tokens,
                    total_tokens, created_at
                ) VALUES (?1, 'codex', 'session-compare', 'path-compare',
                    'project-test', ?2, '2026-05-01T00:00:00Z', ?3,
                    ?4, ?5, ?6, 1, 100, 10, 0, 50, 0, 160, '2026-05-01T00:00:00Z')
                "#,
                rusqlite::params![
                    format!("turn:{event_key}"),
                    model,
                    category,
                    has_edits,
                    retries,
                    one_shot
                ],
            )?;
            conn.execute(
                r#"
                INSERT INTO usage_tool_call(
                    tool_call_key, turn_key, event_key, source, session_id,
                    source_path_hash, project_hash, model, occurred_at, tool_name,
                    tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
                ) VALUES (?1, ?2, ?3, 'codex', 'session-compare', 'path-compare',
                    'project-test', ?4, '2026-05-01T00:00:00Z', 'Edit', 'edit',
                    NULL, NULL, ?5, 'Edit src/lib.rs', '2026-05-01T00:00:00Z')
                "#,
                rusqlite::params![
                    format!("tool:{event_key}"),
                    format!("turn:{event_key}"),
                    event_key,
                    model,
                    format!("fp-{index}")
                ],
            )?;
        }

        let dashboard = Dashboard::open(fixture.store())?;
        let candidates = dashboard.compare_models(&QueryFilter::default())?;
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(|candidate| candidate.low_sample));

        let compare =
            dashboard.model_compare(&QueryFilter::default(), Some("gpt-5"), Some("sonnet"))?;
        assert!(compare.support.supported);
        assert_eq!(compare.support.level, "low_sample");
        assert!(
            compare
                .warning
                .as_deref()
                .unwrap_or("")
                .contains("Low sample")
        );
        assert_eq!(compare.model_a.as_ref().unwrap().model, "gpt-5");
        assert_eq!(compare.model_b.as_ref().unwrap().model, "sonnet");
        assert!(
            compare
                .metrics
                .iter()
                .any(|metric| metric.id == "one_shot_rate")
        );
        assert!(
            compare
                .working_style
                .iter()
                .any(|metric| metric.id == "delegation_rate")
        );
        assert!(
            compare
                .category_head_to_head
                .iter()
                .any(|row| row.category == "coding")
        );
        Ok(())
    }

    /// Legacy per-candidate N+1 oracle for the grouped turn query in
    /// `compare_model_candidates`. Mirrors the pre-refactor implementation:
    /// the unchanged bucket top-25 query plus one `usage_turn` aggregate per
    /// candidate model.
    fn legacy_compare_model_candidates(
        conn: &rusqlite::Connection,
        filter: &QueryFilter,
    ) -> Result<Vec<super::CompareModelCandidate>> {
        let bucket_filter = filter.bucket_filter(Some("b"));
        let sql = format!(
            r#"
            SELECT
                b.model,
                COALESCE(SUM(b.event_count), 0) AS calls,
                COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
                COALESCE(SUM(b.cost_with_cache_usd), 0.0) AS estimated_cost_usd
            FROM usage_bucket_30m b
            {}
            GROUP BY b.model
            ORDER BY estimated_cost_usd DESC, total_tokens DESC, calls DESC, b.model ASC
            LIMIT 25
            "#,
            bucket_filter.where_sql()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(bucket_filter.params().iter()),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<i64>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<f64>>(3)?.unwrap_or_default(),
                ))
            },
        )?;
        let mut candidates = Vec::new();
        for row in rows {
            let (model, calls, total_tokens, estimated_cost_usd) = row?;
            let mut model_filter = filter.clone();
            model_filter.model = Some(model.clone());
            let turn_filter = model_filter.turn_filter(Some("t"));
            let (turns, edit_turns): (i64, i64) = conn.query_row(
                &format!(
                    "SELECT COUNT(*), COALESCE(SUM(t.has_edits), 0) FROM usage_turn t{}",
                    turn_filter.where_sql()
                ),
                rusqlite::params_from_iter(turn_filter.params().iter()),
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            candidates.push(super::CompareModelCandidate {
                model,
                calls,
                turns,
                edit_turns,
                total_tokens,
                estimated_cost_usd,
                low_sample: calls < 20 || edit_turns < 10,
            });
        }
        Ok(candidates)
    }

    /// Equivalence oracle: the grouped turn query must produce byte-identical
    /// candidate JSON to the legacy N+1 across empty, partial-turn, full and
    /// filtered scenarios.
    #[test]
    fn compare_candidates_match_legacy_n_plus_one_output() -> Result<()> {
        // Empty database: no candidates at all.
        let empty = Fixture::new()?;
        let empty_dashboard = Dashboard::open(empty.store())?;
        let new_empty = empty_dashboard.compare_models(&QueryFilter::default())?;
        let legacy_empty =
            legacy_compare_model_candidates(&empty_dashboard.conn, &QueryFilter::default())?;
        assert!(new_empty.is_empty() && legacy_empty.is_empty());

        // 25+ models (LIMIT 25 binds), one of them with buckets but zero
        // turns, plus per-model turn counts that differ.
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(0, 0, 30)?;
        let conn = fixture.store().open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            ) VALUES ('codex', 'no-turns-model', '2026-05-01T00:00:00Z', 'project-stress', 'Project Stress', NULL,
                100, 0, 0, 50, 0, 150, 9.99, 9.99, 'static', 'static-v1',
                6, '2026-05-05T00:00:00Z')
            "#,
            [],
        )?;
        drop(conn);

        let dashboard = Dashboard::open(fixture.store())?;
        let filters = [
            QueryFilter::default(),
            QueryFilter {
                source: Some(SourceKind::Codex),
                ..Default::default()
            },
            QueryFilter {
                model: Some("stress-model-03".to_string()),
                ..Default::default()
            },
            QueryFilter {
                project_hash: Some("project-stress".to_string()),
                ..Default::default()
            },
            QueryFilter {
                project_hash: Some("project-absent".to_string()),
                ..Default::default()
            },
            QueryFilter {
                since: Some(NaiveDate::from_ymd_opt(2026, 5, 2).expect("valid date")),
                until: Some(NaiveDate::from_ymd_opt(2026, 5, 3).expect("valid date")),
                timezone: ReportTimezone::Utc,
                ..Default::default()
            },
        ];
        for filter in &filters {
            let new_candidates = dashboard.compare_models(filter)?;
            let legacy_candidates = legacy_compare_model_candidates(&dashboard.conn, filter)?;
            assert_eq!(
                serde_json::to_value(&new_candidates)?,
                serde_json::to_value(&legacy_candidates)?,
                "candidate JSON must match the legacy N+1 output for filter {filter:?}"
            );
        }
        // The grouped result must contain the no-turns model with zeroed turn
        // stats (top bucket cost puts it first), matching legacy semantics.
        let candidates = dashboard.compare_models(&QueryFilter::default())?;
        let no_turns = candidates
            .iter()
            .find(|candidate| candidate.model == "no-turns-model")
            .expect("bucket-only model is a candidate");
        assert_eq!((no_turns.turns, no_turns.edit_turns), (0, 0));
        assert!(no_turns.low_sample);
        // And the full /api/compare payload stays field-equivalent to a
        // payload whose candidates come from the legacy oracle.
        let full = dashboard.model_compare(&QueryFilter::default(), None, None)?;
        let mut legacy_full = serde_json::to_value(&full)?;
        legacy_full["candidates"] = serde_json::to_value(legacy_compare_model_candidates(
            &dashboard.conn,
            &QueryFilter::default(),
        )?)?;
        assert_eq!(serde_json::to_value(&full)?, legacy_full);
        Ok(())
    }

    static COMPARE_TURN_STATEMENTS: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    fn count_compare_turn_statements(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event
            && sql.contains("usage_turn")
        {
            COMPARE_TURN_STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// The grouped turn query runs exactly once regardless of candidate count
    /// (previously once per candidate, up to 25).
    #[test]
    fn compare_candidates_use_constant_turn_query_count() -> Result<()> {
        for models in [2_usize, 25] {
            let fixture = Fixture::new()?;
            fixture.seed_stress_dashboard(0, 0, models)?;
            let dashboard = Dashboard::open(fixture.store())?;
            dashboard.conn.trace_v2(
                rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
                Some(count_compare_turn_statements),
            );
            COMPARE_TURN_STATEMENTS.store(0, std::sync::atomic::Ordering::Relaxed);
            let candidates = dashboard.compare_models(&QueryFilter::default())?;
            assert_eq!(candidates.len(), models);
            assert_eq!(
                COMPARE_TURN_STATEMENTS.load(std::sync::atomic::Ordering::Relaxed),
                1,
                "compare candidates must run exactly one usage_turn query with {models} models"
            );
            dashboard
                .conn
                .trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, None);
        }
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
        assert_eq!(row.output_tokens, 50);
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
    /// cost columns using the embedded catalog, so a `usage_event` seeded
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
        assert_eq!(source, "static-v2");
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
        assert_eq!(bucket_source, "static-v2");
        Ok(())
    }

    #[test]
    fn recompute_costs_prices_codex_and_claude_cache_channels() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "codex:cache",
            source: "codex",
            model: "gpt-5.5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 1_000_000,
            cache_read_tokens: 2_000_000,
            output_tokens: 3_000_000,
            reasoning_output_tokens: 4_000_000,
            total_tokens: 10_000_000,
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;
        fixture.seed_event(crate::testing::SeedEvent {
            event_key: "claude:cache",
            source: "claude",
            model: "claude-sonnet-4-5",
            event_at: "2026-05-01T00:00:00Z",
            hour_start: Some("2026-05-01T00:00:00Z"),
            input_tokens: 1_000_000,
            cache_read_tokens: 2_000_000,
            cache_creation_tokens: 3_000_000,
            output_tokens: 4_000_000,
            reasoning_output_tokens: 5_000_000,
            total_tokens: 15_000_000,
            created_at: Some("2026-05-01T00:00:00Z"),
            ..Default::default()
        })?;

        let updated = fixture.store().recompute_costs()?;
        assert_eq!(updated, 2);

        let conn = fixture.store().open_connection()?;
        let (codex_cost, codex_without, codex_status): (f64, f64, String) = conn.query_row(
            r#"
            SELECT cost_with_cache_usd, cost_without_cache_usd, pricing_status
            FROM usage_event
            WHERE event_key = 'codex:cache'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert!((codex_cost - 31.5).abs() < EPSILON);
        assert!((codex_without - 33.75).abs() < EPSILON);
        assert_eq!(codex_status, "static");

        let (claude_cost, claude_without, claude_status): (f64, f64, String) = conn.query_row(
            r#"
            SELECT cost_with_cache_usd, cost_without_cache_usd, pricing_status
            FROM usage_event
            WHERE event_key = 'claude:cache'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert!((claude_cost - 72.6).abs() < EPSILON);
        assert!((claude_without - 78.0).abs() < EPSILON);
        assert_eq!(claude_status, "static");

        let (bucket_cost, bucket_tokens, bucket_status): (f64, i64, String) = conn.query_row(
            r#"
            SELECT cost_with_cache_usd, total_tokens, pricing_status
            FROM usage_bucket_30m
            WHERE source = 'claude' AND model = 'claude-sonnet-4-5'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        assert!((bucket_cost - claude_cost).abs() < EPSILON);
        assert_eq!(bucket_tokens, 15_000_000);
        assert_eq!(bucket_status, "static");

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
        const LIVE_BUCKETS: usize = 128;
        for index in 0..LIVE_BUCKETS {
            let event_key = format!("codex:live-bucket-{index:03}");
            let day = index / 24 + 1;
            let hour = index % 24;
            let event_at = format!("2026-05-{day:02}T{hour:02}:00:00Z");
            fixture.seed_event(crate::testing::SeedEvent {
                event_key: &event_key,
                source: "codex",
                model: "gpt-5",
                event_at: &event_at,
                hour_start: Some(&event_at),
                input_tokens: 1_000,
                output_tokens: 500,
                total_tokens: 1_500,
                created_at: Some(&event_at),
                ..Default::default()
            })?;
        }
        let conn = fixture.store().open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate,
                event_count, updated_at
            ) VALUES ('codex', 'gpt-5', '2026-06-01T00:00:00Z', '', NULL, NULL,
                0, 0, 0, 0, 0, 0,
                42.0, 42.0, 'static', 'static-v1', '{}',
                0, '2026-06-01T00:00:00Z')
            "#,
            [],
        )?;

        let before: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(before, LIVE_BUCKETS as i64 + 1);

        let updated = fixture.store().recompute_costs()?;
        assert_eq!(updated, LIVE_BUCKETS);

        let after: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_bucket_30m WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            after, LIVE_BUCKETS as i64,
            "orphan bucket should be deleted without affecting live buckets"
        );
        let event_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE source = 'codex'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(event_count, LIVE_BUCKETS as i64);
        let orphan_count: i64 = conn.query_row(
            r#"
            SELECT COUNT(*) FROM usage_bucket_30m
            WHERE source = 'codex' AND model = 'gpt-5' AND hour_start = '2026-06-01T00:00:00Z'
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

        for source in ["claude", "codex", "antigravity", "opencode"] {
            assert!(payload.by_platform.contains_key(source));
        }
        assert!(payload.by_platform["codex"].requests > 0);
        assert!(payload.by_platform["claude"].requests > 0);
        assert!(payload.by_platform["opencode"].requests > 0);
        assert_eq!(payload.by_platform["antigravity"].requests, 0);
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
    fn home_overview_preserves_exact_session_day_and_filter_semantics() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:cross-day:1",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-04-01T23:30:00Z",
            hour_start: Some("2026-04-01T23:00:00Z"),
            session_id: Some("session-shared"),
            project_hash: "project-a",
            input_tokens: 10,
            total_tokens: 10,
            ..Default::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "codex:cross-day:2",
            source: "codex",
            model: "gpt-5",
            event_at: "2026-04-02T00:30:00Z",
            hour_start: Some("2026-04-02T00:00:00Z"),
            session_id: Some("session-shared"),
            project_hash: "project-a",
            input_tokens: 20,
            total_tokens: 20,
            ..Default::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "claude:fallback:1",
            source: "claude",
            model: "claude-sonnet-4",
            event_at: "2026-04-02T01:00:00Z",
            hour_start: Some("2026-04-02T01:00:00Z"),
            source_path_hash: Some("claude-path"),
            project_hash: "project-b",
            input_tokens: 30,
            total_tokens: 30,
            ..Default::default()
        })?;
        fixture.seed_event(SeedEvent {
            event_key: "opencode:fallback:1",
            source: "opencode",
            model: "gpt-5",
            event_at: "2026-04-02T16:00:00Z",
            hour_start: Some("2026-04-02T16:00:00Z"),
            source_path_hash: Some("opencode-path"),
            project_hash: "project-c",
            input_tokens: 40,
            total_tokens: 40,
            ..Default::default()
        })?;
        fixture.store().open_connection()?.execute(
            "INSERT INTO run_log(command, status, started_at, finished_at) VALUES ('sync', 'success', '2026-04-03T00:00:00Z', '2026-04-03T00:01:00Z')",
            [],
        )?;

        let dashboard = Dashboard::open(fixture.store())?;
        let utc_plus_eight = FixedOffset::east_opt(8 * 60 * 60).expect("valid offset");
        let all_filter = QueryFilter {
            timezone: ReportTimezone::Fixed(utc_plus_eight),
            ..Default::default()
        };
        let payload = dashboard.home_overview(&all_filter)?;
        let (profiled_payload, _) = home_overview::load_profile(&dashboard, &all_filter)?;
        assert_eq!(
            serde_json::to_value(&payload)?,
            serde_json::to_value(&profiled_payload)?
        );
        assert_eq!(payload.summary.total_sessions, 3);
        assert_eq!(payload.summary.total_requests, 4);
        assert_eq!(payload.summary.total_tokens, 100);
        assert_eq!(payload.summary.active_days, 2);
        assert_eq!(payload.by_platform["codex"].sessions, 1);
        assert_eq!(payload.by_platform["codex"].requests, 2);
        assert_eq!(payload.by_platform["claude"].sessions, 1);
        assert_eq!(payload.by_platform["opencode"].sessions, 1);
        assert_eq!(payload.series.len(), 2);
        assert_eq!(payload.series[0].date, "2026-04-02");
        assert_eq!(payload.series[0].codex.sessions, 1);
        assert_eq!(payload.series[0].codex.requests, 2);
        assert_eq!(payload.series[0].claude.sessions, 1);
        assert_eq!(payload.series[1].date, "2026-04-03");
        assert_eq!(payload.series[1].opencode.sessions, 1);
        assert!(payload.bootstrap.usage_import_attempted);
        assert!(payload.bootstrap.is_warm);
        assert_eq!(payload.last_updated, "2026-04-03T00:01:00Z");

        let filtered = dashboard.home_overview(&QueryFilter {
            source: Some(SourceKind::Codex),
            model: Some("gpt-5".to_string()),
            project_hash: Some("project-a".to_string()),
            since: Some(NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date")),
            until: Some(NaiveDate::from_ymd_opt(2026, 4, 2).expect("valid date")),
            timezone: ReportTimezone::Fixed(utc_plus_eight),
        })?;
        assert_eq!(filtered.summary.total_sessions, 1);
        assert_eq!(filtered.summary.total_requests, 2);
        assert_eq!(filtered.summary.total_tokens, 30);
        assert_eq!(filtered.summary.active_days, 1);
        assert_eq!(filtered.by_platform["codex"].requests, 2);
        assert_eq!(filtered.by_platform["claude"].requests, 0);
        assert_eq!(filtered.series.len(), 1);
        assert_eq!(filtered.series[0].date, "2026-04-02");
        assert_eq!(filtered.series[0].codex.sessions, 1);
        Ok(())
    }

    #[test]
    fn home_overview_under_80ms_with_seeded_10k_events() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_dashboard(10_000)?;
        let dashboard = Dashboard::open(fixture.store())?;
        let started = std::time::Instant::now();

        let (payload, timing) = home_overview::load_profile(&dashboard, &Default::default())?;
        eprintln!("home_overview profile: {timing:?}");
        let elapsed = started.elapsed();
        let limit = if std::env::var_os("CI").is_some() {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_millis(80)
        };

        assert_eq!(payload.summary.total_requests, 10_000);
        assert!(
            elapsed < limit,
            "home_overview should stay below {limit:?} with 10k seeded events, got {elapsed:?}"
        );
        Ok(())
    }

    #[test]
    fn home_overview_profiles_configured_read_only_backup() -> Result<()> {
        let Some(db_path) = std::env::var_os("LLMUSAGE_HOME_OVERVIEW_BACKUP_DB") else {
            return Ok(());
        };
        let db_path = std::path::PathBuf::from(db_path);
        let conn = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let event_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_event", [], |row| row.get(0))?;
        let bucket_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_bucket_30m", [], |row| {
                row.get(0)
            })?;
        drop(conn);

        for run in 0..5 {
            let (_, timing) = home_overview::load_profile_read_only(&db_path, &Default::default())?;
            eprintln!(
                "home_overview real backup run={run} events={event_count} buckets={bucket_count} timing={timing:?}"
            );
        }
        Ok(())
    }

    /// Measurement-only baseline for the serve dashboard query path task.
    /// Run explicitly: `cargo test --lib measure_stress_diagnostics_and_full_sections -- --ignored --nocapture --test-threads=1`
    #[test]
    #[ignore = "measurement test; run explicitly for baseline/after reports"]
    fn measure_stress_diagnostics_and_full_sections() -> Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(4_000, 1_000, 25)?;
        let conn = fixture.store().open_connection()?;
        let source_file_rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM source_file", [], |row| row.get(0))?;
        let turn_rows: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_turn", [], |row| row.get(0))?;
        let model_rows: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT model) FROM usage_bucket_30m",
            [],
            |row| row.get(0),
        )?;
        drop(conn);
        eprintln!(
            "stress scale: source_file={source_file_rows} usage_turn={turn_rows} models={model_rows}"
        );

        let dashboard = Dashboard::open(fixture.store())?;
        let filter = QueryFilter::default();
        for run in 0..3 {
            super::reset_diagnostics_stat_counter();
            let started = std::time::Instant::now();
            let diagnostics = dashboard.diagnostics()?;
            eprintln!(
                "diagnostics run={run} elapsed={:?} stat_calls={} by_source={}",
                started.elapsed(),
                super::diagnostics_stat_calls(),
                diagnostics.by_source.len()
            );
        }

        #[allow(clippy::type_complexity)]
        let timed: [(&str, &dyn Fn(&Dashboard) -> crate::error::Result<()>); 7] = [
            ("core_snapshot", &|d| d.core_snapshot(&filter).map(|_| ())),
            ("activity", &|d| d.activity_breakdown(&filter).map(|_| ())),
            ("tools", &|d| d.tool_breakdown(&filter).map(|_| ())),
            ("optimize", &|d| d.optimize(&filter).map(|_| ())),
            ("compare", &|d| {
                d.model_compare(&filter, None, None).map(|_| ())
            }),
            ("explorer", &|d| {
                d.explorer(&super::ExplorerQuery {
                    filter: filter.clone(),
                    ..Default::default()
                })
                .map(|_| ())
            }),
            ("full_snapshot_export", &|d| d.snapshot(&filter).map(|_| ())),
        ];
        for (section, run_fn) in timed {
            let started = std::time::Instant::now();
            run_fn(&dashboard)?;
            eprintln!("section {section} elapsed={:?}", started.elapsed());
        }
        Ok(())
    }

    /// Measurement against a read-only copy of a representative database.
    /// Set `LLMUSAGE_MEASURE_HOME` to the copied runtime root (the directory
    /// that contains `llmusage.db`) and run with `--ignored --nocapture`.
    #[test]
    #[ignore = "measurement test; requires LLMUSAGE_MEASURE_HOME copy"]
    fn measure_real_copy_diagnostics_and_full_sections() -> Result<()> {
        let Some(root) = std::env::var_os("LLMUSAGE_MEASURE_HOME") else {
            eprintln!("LLMUSAGE_MEASURE_HOME not set; skipping real-copy measurement");
            return Ok(());
        };
        let paths = crate::paths::AppPaths::with_root(std::path::PathBuf::from(root))?;
        let db_path = paths.db_path.clone();
        let conn = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let scale: Vec<(&str, i64)> = [
            "source_file",
            "usage_event",
            "usage_bucket_30m",
            "usage_turn",
        ]
        .iter()
        .map(|table| {
            let count = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })?;
            Ok((*table, count))
        })
        .collect::<Result<Vec<_>>>()?;
        let models: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT model) FROM usage_bucket_30m",
            [],
            |row| row.get(0),
        )?;
        eprintln!("real copy scale: {scale:?} models={models}");

        for run in 0..5 {
            super::reset_diagnostics_stat_counter();
            let started = std::time::Instant::now();
            let rows = super::load_source_diagnostics(&conn)?;
            eprintln!(
                "real diagnostics run={run} elapsed={:?} stat_calls={} by_source={}",
                started.elapsed(),
                super::diagnostics_stat_calls(),
                rows.len()
            );
        }
        drop(conn);

        // Section breakdown through the normal Dashboard facade. This is a
        // file copy, so opening it read-write here never touches the original.
        let store = crate::store::Store::new(&paths)?;
        let dashboard = Dashboard::open(&store)?;
        let filter = QueryFilter::default();
        #[allow(clippy::type_complexity)]
        let timed: [(&str, &dyn Fn(&Dashboard) -> crate::error::Result<()>); 6] = [
            ("core_snapshot", &|d| d.core_snapshot(&filter).map(|_| ())),
            ("activity", &|d| d.activity_breakdown(&filter).map(|_| ())),
            ("tools", &|d| d.tool_breakdown(&filter).map(|_| ())),
            ("optimize", &|d| d.optimize(&filter).map(|_| ())),
            ("compare", &|d| {
                d.model_compare(&filter, None, None).map(|_| ())
            }),
            ("explorer", &|d| {
                d.explorer(&super::ExplorerQuery {
                    filter: filter.clone(),
                    ..Default::default()
                })
                .map(|_| ())
            }),
        ];
        for (section, run_fn) in timed {
            let started = std::time::Instant::now();
            run_fn(&dashboard)?;
            eprintln!("real section {section} elapsed={:?}", started.elapsed());
        }
        Ok(())
    }
}
