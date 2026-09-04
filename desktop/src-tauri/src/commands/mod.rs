pub mod jobs;
pub mod query;
pub mod runtime;

use tauri::State;

use crate::{
    dto::{
        CancelQueriesDto, ExplorerDto, InteractiveRequest, LogsDto, PrefsDto, QuotaResponse,
        RuntimeInfoDto, SecondaryRequest, SyncStartDto, TopSessionsDto,
    },
    error::DesktopError,
    state::AppState,
};

#[tauri::command]
pub async fn runtime_info(state: State<'_, AppState>) -> Result<RuntimeInfoDto, DesktopError> {
    runtime::runtime_info(&state)
}

#[tauri::command]
pub async fn dashboard_interactive(
    state: State<'_, AppState>,
    request: InteractiveRequest,
) -> Result<llmusage::query::DashboardInteractiveSnapshot, DesktopError> {
    query::dashboard_interactive(&state, request).await
}

#[tauri::command]
pub async fn home_overview(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<llmusage::HomeOverviewPayload, DesktopError> {
    query::home_overview(&state, request).await
}

#[tauri::command]
pub async fn heatmap(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<Vec<llmusage::query::HeatmapPoint>, DesktopError> {
    query::heatmap(&state, request).await
}

#[tauri::command]
pub async fn trends_daily(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<Vec<llmusage::DailyTrendPoint>, DesktopError> {
    query::trends_daily(&state, request).await
}

#[tauri::command]
pub async fn hour_of_week(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<Vec<llmusage::query::HourOfWeekCell>, DesktopError> {
    query::hour_of_week(&state, request).await
}

#[tauri::command]
pub async fn top_sessions(
    state: State<'_, AppState>,
    request: TopSessionsDto,
) -> Result<Vec<llmusage::TopSessionRow>, DesktopError> {
    query::top_sessions(&state, request).await
}

#[tauri::command]
pub async fn activity(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<llmusage::ActivityPayload, DesktopError> {
    query::activity(&state, request).await
}

#[tauri::command]
pub async fn tools(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<llmusage::ToolsPayload, DesktopError> {
    query::tools(&state, request).await
}

#[tauri::command]
pub async fn optimize(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<llmusage::OptimizePayload, DesktopError> {
    query::optimize(&state, request).await
}

#[tauri::command]
pub async fn compare(
    state: State<'_, AppState>,
    request: SecondaryRequest,
) -> Result<llmusage::ModelComparePayload, DesktopError> {
    query::compare(&state, request).await
}

#[tauri::command]
pub async fn explorer(
    state: State<'_, AppState>,
    request: ExplorerDto,
) -> Result<llmusage::ExplorerPayload, DesktopError> {
    query::explorer(&state, request).await
}

#[tauri::command]
pub async fn logs(
    state: State<'_, AppState>,
    request: LogsDto,
) -> Result<llmusage::LogsPage, DesktopError> {
    query::logs(&state, request).await
}

#[tauri::command]
pub async fn diagnostics(
    state: State<'_, AppState>,
) -> Result<llmusage::DiagnosticsPayload, DesktopError> {
    query::diagnostics(&state).await
}

#[tauri::command]
pub async fn start_sync(
    state: State<'_, AppState>,
    request: SyncStartDto,
) -> Result<llmusage::JobSnapshot, DesktopError> {
    jobs::start_sync(&state, request)
}

#[tauri::command]
pub async fn job_snapshot(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<llmusage::JobSnapshot>, DesktopError> {
    Ok(jobs::job_snapshot(&state, id))
}

#[tauri::command]
pub async fn cancel_job(state: State<'_, AppState>, id: String) -> Result<bool, DesktopError> {
    Ok(jobs::cancel_job(&state, id))
}

#[tauri::command]
pub async fn cancel_queries(
    state: State<'_, AppState>,
    request: CancelQueriesDto,
) -> Result<(), DesktopError> {
    runtime::cancel_queries(&state, request);
    Ok(())
}

#[tauri::command]
pub async fn fetch_quota(
    state: State<'_, AppState>,
    bypass_cache: bool,
) -> Result<QuotaResponse, DesktopError> {
    runtime::fetch_quota(&state, bypass_cache, runtime::QuotaInject::default()).await
}

#[tauri::command]
pub async fn load_prefs(state: State<'_, AppState>) -> Result<PrefsDto, DesktopError> {
    runtime::load_prefs(&state)
}

#[tauri::command]
pub async fn save_prefs(
    state: State<'_, AppState>,
    prefs: PrefsDto,
) -> Result<PrefsDto, DesktopError> {
    runtime::save_prefs(&state, prefs)
}
