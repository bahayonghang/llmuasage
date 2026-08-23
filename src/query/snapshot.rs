use super::*;

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
    /// Per-host breakdown table.
    pub hosts: Vec<HostBreakdown>,
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
    /// Default Usage analysis slice captured for live dashboard bootstrap and
    /// static HTML exports.
    pub explorer: ExplorerPayload,
    /// Integration/cursor/run health payload.
    pub health: HealthPayload,
    /// Archive/source-file diagnostics plus recent failed run records.
    pub diagnostics: DiagnosticsPayload,
    /// Compact home overview data used by the summary card row.
    pub home_overview: Option<HomeOverviewSnapshot>,
    /// Calendar activity for the most recent 366 days.
    pub heatmap: Option<Vec<HeatmapPoint>>,
    /// Per-day token breakdown used by the stacked daily chart.
    pub trends_daily: Option<Vec<DailyTrendPoint>>,
    /// Top sessions ranked by total tokens for offline analytics.
    pub top_sessions: Option<Vec<TopSessionRow>>,
    /// Local-time 7x24 activity grid for offline analytics.
    pub hour_of_week: Option<Vec<HourOfWeekCell>>,
}

/// Snapshot-only projection of [`HomeOverviewPayload`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeOverviewSnapshot {
    pub summary: HomeOverviewSummary,
    pub by_platform: BTreeMap<String, HomeOverviewPlatformStats>,
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
    /// Per-host cost/token table.
    pub hosts: Vec<HostBreakdown>,
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
    pub hosts: Vec<HostBreakdown>,
    pub projects: Vec<ProjectBreakdown>,
    pub costs: Vec<CostLine>,
    pub health: HealthSummaryPayload,
    pub diagnostics: DiagnosticsPayload,
}

impl Dashboard {
    /// Loads the ccr-ui home overview payload from the same dashboard connection.
    pub fn home_overview(&self, filter: &QueryFilter) -> Result<HomeOverviewPayload> {
        home_overview::load(self, filter)
    }

    /// Loads the summary-card projection without the full overview's series,
    /// run-state, or diagnostics work.
    pub fn home_overview_compact(&self, filter: &QueryFilter) -> Result<HomeOverviewSnapshot> {
        home_overview::load_compact(self, filter)
    }

    /// Loads a `days`-day activity heatmap (F4.3) ending at the explicit
    /// [`QueryFilter::until`] date, or today in [`QueryFilter::timezone`]
    /// when the filter has no upper bound. Days without activity are
    /// zero-filled; values are clamped to a 1..=366 window.
    pub fn heatmap(&self, filter: &QueryFilter, days: u32) -> Result<Vec<HeatmapPoint>> {
        heatmap::load(self, filter, days)
    }

    /// Loads the flexible Usage analysis aggregate for the requested slice.
    pub fn explorer(&self, query: &ExplorerQuery) -> Result<ExplorerPayload> {
        explorer::load(self, query)
    }

    /// Loads cursor-paginated usage log rows (F4.3 / D26).
    pub fn logs(&self, query: &LogsQuery) -> Result<LogsPage> {
        logs::load(self, query)
    }

    /// Loads a stable, server-ranked Top Sessions list.
    pub fn top_sessions(&self, query: &TopSessionsQuery) -> Result<Vec<TopSessionRow>> {
        top_sessions::load(self, query)
    }

    /// Loads a zero-filled Monday-first 7x24 activity grid.
    pub fn hour_of_week(&self, filter: &QueryFilter) -> Result<Vec<HourOfWeekCell>> {
        hour_of_week::load(self, filter)
    }

    /// Builds the full dashboard snapshot used by static HTML export.
    ///
    /// The snapshot still embeds the legacy four-window trends (`day`/`week`/
    /// `month`/`all`) for backwards-compat HTML export. It intentionally uses
    /// the legacy scalar trend shape because `/api/trends?window=` still
    /// exposes that contract.
    pub fn snapshot(&self, filter: &QueryFilter) -> Result<DashboardSnapshot> {
        let core = self.core_snapshot(filter)?;
        let home_overview = self.home_overview_compact(filter)?;
        Ok(DashboardSnapshot {
            overview: core.overview,
            sync_command_center: core.sync_command_center,
            day_trends: core.day_trends,
            week_trends: core.week_trends,
            month_trends: core.month_trends,
            all_trends: core.all_trends,
            models: core.models,
            sources: core.sources,
            hosts: core.hosts,
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
            home_overview: Some(home_overview),
            heatmap: Some(self.heatmap(filter, 366)?),
            trends_daily: Some(self.trends_daily(filter)?),
            top_sessions: Some(self.top_sessions(&TopSessionsQuery {
                filter: filter.clone(),
                ..TopSessionsQuery::default()
            })?),
            hour_of_week: Some(self.hour_of_week(filter)?),
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
            hosts: self.host_breakdown(filter)?,
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
            hosts: self.host_breakdown(filter)?,
            projects: self.project_breakdown(filter)?,
            costs: self.cost_breakdown(filter)?,
            health: self.health_summary()?,
            diagnostics: diagnostics.clone(),
        })
    }
}
