use std::time::Duration;

use llmusage::query::{
    DailyTrendPoint, DashboardInteractiveSnapshot, HeatmapPoint, HomeOverviewPayload,
    HourOfWeekCell,
};
use llmusage::{
    ActivityPayload, BehaviorSupport, Dashboard, ExplorerPayload, LogsPage, ModelComparePayload,
    OptimizePayload, ToolsPayload, TopSessionRow,
};

use crate::{
    dto::{ExplorerDto, InteractiveRequest, LogsDto, SecondaryRequest, TopSessionsDto},
    error::DesktopError,
    state::AppState,
    supervisor::run_query,
};

const INTERACTIVE_TIMEOUT: Duration = Duration::from_secs(6);
const SECONDARY_TIMEOUT: Duration = Duration::from_secs(5);
const BEHAVIOR_TIMEOUT: Duration = Duration::from_secs(3);
const HEATMAP_DAYS: u32 = 365;

pub async fn dashboard_interactive(
    state: &AppState,
    request: InteractiveRequest,
) -> Result<DashboardInteractiveSnapshot, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    let window = crate::dto::convert_window(&request.window)?;
    let diagnostics = state.load_diagnostics_cached().await?;
    run_query(
        state,
        request.request_id,
        INTERACTIVE_TIMEOUT,
        move |dashboard| {
            dashboard.interactive_snapshot_with_diagnostics(&filter, &window, &diagnostics)
        },
    )
    .await
}

pub async fn home_overview(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<HomeOverviewPayload, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    run_query(
        state,
        request.request_id,
        SECONDARY_TIMEOUT,
        move |dashboard| dashboard.home_overview(&filter),
    )
    .await
}

pub async fn heatmap(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<Vec<HeatmapPoint>, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    run_query(
        state,
        request.request_id,
        SECONDARY_TIMEOUT,
        move |dashboard| dashboard.heatmap(&filter, HEATMAP_DAYS),
    )
    .await
}

pub async fn trends_daily(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<Vec<DailyTrendPoint>, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    run_query(
        state,
        request.request_id,
        SECONDARY_TIMEOUT,
        move |dashboard| dashboard.trends_daily(&filter),
    )
    .await
}

pub async fn hour_of_week(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<Vec<HourOfWeekCell>, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    run_query(
        state,
        request.request_id,
        SECONDARY_TIMEOUT,
        move |dashboard| dashboard.hour_of_week(&filter),
    )
    .await
}

pub async fn top_sessions(
    state: &AppState,
    request: TopSessionsDto,
) -> Result<Vec<TopSessionRow>, DesktopError> {
    let (_, query) = crate::dto::convert_top_sessions(&request)?;
    run_query(
        state,
        request.request_id,
        SECONDARY_TIMEOUT,
        move |dashboard| dashboard.top_sessions(&query),
    )
    .await
}

pub async fn activity(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<ActivityPayload, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    behavior(
        state,
        request.request_id,
        move |dashboard| dashboard.activity_breakdown(&filter),
        degraded_activity,
    )
    .await
}

pub async fn tools(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<ToolsPayload, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    behavior(
        state,
        request.request_id,
        move |dashboard| dashboard.tool_breakdown(&filter),
        degraded_tools,
    )
    .await
}

pub async fn optimize(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<OptimizePayload, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    behavior(
        state,
        request.request_id,
        move |dashboard| dashboard.optimize(&filter),
        degraded_optimize,
    )
    .await
}

pub async fn compare(
    state: &AppState,
    request: SecondaryRequest,
) -> Result<ModelComparePayload, DesktopError> {
    let filter = crate::dto::convert_filter(&request.filter)?;
    behavior(
        state,
        request.request_id,
        move |dashboard| dashboard.model_compare(&filter, None, None),
        degraded_compare,
    )
    .await
}

pub async fn explorer(
    state: &AppState,
    request: ExplorerDto,
) -> Result<ExplorerPayload, DesktopError> {
    let (request_id, query) = crate::dto::convert_explorer(&request)?;
    run_query(state, request_id, SECONDARY_TIMEOUT, move |dashboard| {
        dashboard.explorer(&query)
    })
    .await
}

pub async fn logs(state: &AppState, request: LogsDto) -> Result<LogsPage, DesktopError> {
    let (request_id, query) = crate::dto::convert_logs(&request)?;
    run_query(state, request_id, SECONDARY_TIMEOUT, move |dashboard| {
        dashboard.logs(&query)
    })
    .await
}

pub async fn diagnostics(state: &AppState) -> Result<llmusage::DiagnosticsPayload, DesktopError> {
    state.load_diagnostics_cached().await
}

async fn behavior<T, F, D>(
    state: &AppState,
    request_id: u64,
    f: F,
    degraded: D,
) -> Result<T, DesktopError>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> llmusage::Result<T> + Send + 'static,
    D: FnOnce(String) -> T,
{
    match run_query(state, request_id, BEHAVIOR_TIMEOUT, f).await {
        Ok(value) => Ok(value),
        Err(error) => Ok(degraded(error.message)),
    }
}

fn degraded_support(reason: String) -> BehaviorSupport {
    BehaviorSupport {
        supported: false,
        level: "degraded".to_string(),
        reason: Some(reason),
    }
}

fn degraded_activity(reason: String) -> ActivityPayload {
    ActivityPayload {
        support: degraded_support(reason),
        breakdown: Vec::new(),
    }
}

fn degraded_tools(reason: String) -> ToolsPayload {
    ToolsPayload {
        support: degraded_support(reason),
        breakdown: Vec::new(),
    }
}

fn degraded_optimize(reason: String) -> OptimizePayload {
    OptimizePayload {
        support: degraded_support(reason),
        score: 100,
        grade: "A".to_string(),
        estimated_savings_tokens: 0,
        estimated_savings_usd: 0.0,
        findings: Vec::new(),
    }
}

fn degraded_compare(reason: String) -> ModelComparePayload {
    ModelComparePayload {
        support: degraded_support(reason.clone()),
        candidates: Vec::new(),
        model_a: None,
        model_b: None,
        metrics: Vec::new(),
        category_head_to_head: Vec::new(),
        working_style: Vec::new(),
        warning: Some(reason),
    }
}
