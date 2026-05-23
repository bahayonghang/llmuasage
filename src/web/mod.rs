use std::{collections::HashMap, future::Future, net::SocketAddr, time::Duration};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use chrono::{FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::{
    error::{LlmusageError, Result as LlmusageResult},
    models::SourceKind,
    query::{
        ActivityPayload, BehaviorSupport, Dashboard, LogsQuery, ModelComparePayload,
        OptimizePayload, QueryFilter, ToolsPayload,
    },
    store::Store,
    sync::{JobRegistry, SyncOptions},
};

const WEB_API_TIMEOUT: Duration = Duration::from_secs(5);
const WEB_BEHAVIOR_API_TIMEOUT: Duration = Duration::from_secs(1);

mod assets;
mod brand;
mod shell;

#[derive(Clone)]
pub struct WebState {
    pub store: Store,
    pub jobs: JobRegistry,
}

pub async fn serve(store: Store, preferred_port: Option<u16>) -> Result<SocketAddr> {
    /*
     * ========================================================================
     * 步骤1：组装本地 Web UI 路由
     * ========================================================================
     * 目标：
     * 1) 只暴露根页面、静态资源和既有 JSON API
     * 2) 把静态资源分发统一收敛到 assets manifest
     * 3) 保持 serve 与 export html 共用同一套前端资源
     */
    info!("开始组装本地 Web UI 路由");

    // 1.1 创建状态并收敛根页面、资源和 API 路由
    let state = WebState {
        store,
        jobs: JobRegistry::default(),
    };
    let app = Router::new()
        .route("/", get(index_live))
        .route("/assets/{*path}", get(asset_file))
        .route("/api/dashboard", get(api_dashboard))
        .route("/api/overview", get(api_overview))
        .route("/api/trends", get(api_trends))
        .route("/api/trends_daily", get(api_trends_daily))
        .route("/api/models", get(api_models))
        .route("/api/sources", get(api_sources))
        .route("/api/projects", get(api_projects))
        .route("/api/costs", get(api_costs))
        .route("/api/activity", get(api_activity))
        .route("/api/tools", get(api_tools))
        .route("/api/optimize", get(api_optimize))
        .route("/api/compare/models", get(api_compare_models))
        .route("/api/compare", get(api_compare))
        .route("/api/home_overview", get(api_home_overview))
        .route("/api/heatmap", get(api_heatmap))
        .route("/api/logs", get(api_logs))
        .route("/api/diagnostics", get(api_diagnostics))
        .route("/api/diagnostics/forget", post(api_diagnostics_forget))
        .route("/api/jobs", post(api_jobs_start))
        .route("/api/jobs/{id}", get(api_jobs_get))
        .route("/api/jobs/{id}/cancel", post(api_jobs_cancel))
        .route("/api/health", get(api_health))
        .with_state(state);

    info!("完成本地 Web UI 路由组装");

    /*
     * ========================================================================
     * 步骤2：绑定本地监听端口
     * ========================================================================
     * 目标：
     * 1) 继续只监听 127.0.0.1
     * 2) 复用既有端口探测顺序
     * 3) 命中端口后立即后台启动 axum 服务
     */
    info!("开始绑定本地 Web UI 监听端口");

    // 2.1 根据优先端口或默认端口组探测本地监听地址
    let ports = if let Some(port) = preferred_port {
        vec![port]
    } else {
        vec![37421, 37422, 37423, 0]
    };

    // 2.2 命中可用端口后启动服务并返回最终监听地址
    for port in ports {
        let listener = TcpListener::bind(("127.0.0.1", port)).await;
        if let Ok(listener) = listener {
            let addr = listener.local_addr()?;
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            info!("完成本地 Web UI 监听端口绑定");
            return Ok(addr);
        }
    }

    unreachable!("端口探测列表不应为空");
}

pub fn snapshot_index_html() -> String {
    shell::snapshot_index_html()
}

pub fn live_index_html() -> String {
    shell::live_index_html()
}

pub(crate) fn asset_manifest() -> &'static [assets::WebAsset] {
    assets::asset_manifest()
}

async fn index_live() -> Html<String> {
    Html(live_index_html())
}

async fn asset_file(Path(path): Path<String>) -> Response {
    let normalized = path.trim_start_matches('/');
    match assets::find_asset(normalized) {
        Some(asset) => asset.as_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn api_dashboard(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/dashboard",
        load_dashboard_snapshot_resilient(state, filter),
    )
    .await
}

async fn api_overview(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/overview",
        load_via_dashboard(state, move |d| d.overview(&filter)),
    )
    .await
}

async fn api_trends(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let window = params
        .get("window")
        .cloned()
        .unwrap_or_else(|| "day".to_string());
    let filter = dashboard_filter_from_params_without_window(&params);
    api_json_async(
        "/api/trends",
        load_via_dashboard(state, move |d| d.trends(&window, &filter)),
    )
    .await
}

async fn api_trends_daily(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/trends_daily",
        load_via_dashboard(state, move |d| d.trends_daily(&filter)),
    )
    .await
}

async fn api_models(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/models",
        load_via_dashboard(state, move |d| d.model_breakdown(&filter)),
    )
    .await
}

async fn api_sources(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/sources",
        load_via_dashboard(state, move |d| d.source_breakdown(&filter)),
    )
    .await
}

async fn api_projects(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/projects",
        load_via_dashboard(state, move |d| d.project_breakdown(&filter)),
    )
    .await
}

async fn api_costs(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/costs",
        load_via_dashboard(state, move |d| d.cost_breakdown(&filter)),
    )
    .await
}

async fn api_activity(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/activity",
        load_behavior_api(
            state,
            move |d| d.activity_breakdown(&filter),
            degraded_activity,
        ),
    )
    .await
}

async fn api_tools(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/tools",
        load_behavior_api(state, move |d| d.tool_breakdown(&filter), degraded_tools),
    )
    .await
}

async fn api_optimize(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/optimize",
        load_behavior_api(state, move |d| d.optimize(&filter), degraded_optimize),
    )
    .await
}

async fn api_compare_models(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/compare/models",
        load_via_dashboard(state, move |d| d.compare_models(&filter)),
    )
    .await
}

async fn api_compare(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/compare",
        load_behavior_api(
            state,
            move |d| {
                d.model_compare(
                    &filter,
                    params.get("model_a").map(String::as_str),
                    params.get("model_b").map(String::as_str),
                )
            },
            degraded_compare,
        ),
    )
    .await
}

async fn api_home_overview(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/home_overview",
        load_via_dashboard(state, move |d| d.home_overview(&filter)),
    )
    .await
}

async fn api_heatmap(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let days = params
        .get("days")
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(365);
    let filter = dashboard_filter_from_params(&params);
    api_json_async(
        "/api/heatmap",
        load_via_dashboard(state, move |d| d.heatmap(&filter, days)),
    )
    .await
}

async fn api_logs(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let cursor = params
        .get("cursor")
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty());

    if let Some(cursor) = cursor.as_deref()
        && crate::query::logs::try_decode_cursor(cursor).is_none()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "invalid_cursor",
                    "message": "logs cursor 必须是 base64url(JSON{event_at,event_key})",
                }
            })),
        )
            .into_response();
    }

    let filter = dashboard_filter_from_params(&params);
    let query = LogsQuery {
        filter,
        page_size: params
            .get("page_size")
            .and_then(|raw| raw.parse::<u32>().ok())
            .unwrap_or(50),
        cursor,
        include_total: parse_bool_query(params.get("include_total")),
        include_raw_json: parse_bool_query(
            params
                .get("include_raw")
                .or_else(|| params.get("include_raw_json")),
        ),
    };

    api_json_async(
        "/api/logs",
        load_via_dashboard(state, move |d| d.logs(&query)),
    )
    .await
}

async fn api_health(State(state): State<WebState>) -> Response {
    api_json_async("/api/health", load_via_dashboard(state, |d| d.health())).await
}

async fn api_diagnostics(State(state): State<WebState>) -> Response {
    api_json_async(
        "/api/diagnostics",
        load_via_dashboard(state, |d| d.diagnostics()),
    )
    .await
}

#[derive(Debug, Deserialize)]
struct ForgetRequest {
    /// Absolute file path the user wants llmusage to forget.
    file_path: String,
    /// Source identifier (`codex` / `claude` / `opencode`). Required when the
    /// same path could exist in multiple sources.
    source: Option<String>,
}

async fn api_diagnostics_forget(
    State(state): State<WebState>,
    Json(payload): Json<ForgetRequest>,
) -> Response {
    let Some(source_str) = payload.source.as_deref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "missing_source",
                    "message": "POST /api/diagnostics/forget 必须显式传 source 字段",
                }
            })),
        )
            .into_response();
    };
    let Some(source) = SourceKind::parse_id(source_str) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "code": "unknown_source",
                    "message": format!("未知 source: {source_str}"),
                }
            })),
        )
            .into_response();
    };

    match state
        .store
        .mark_source_file_deleted(source, &payload.file_path)
    {
        Ok(()) => Json(json!({
            "ok": true,
            "source": source.as_str(),
            "file_path": payload.file_path,
        }))
        .into_response(),
        Err(err) => {
            error!(endpoint = "/api/diagnostics/forget", error = %err, "forget failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "code": "internal_error",
                        "message": "登记 forget 失败",
                        "detail": err.to_string(),
                    }
                })),
            )
                .into_response()
        }
    }
}

async fn api_jobs_start(
    State(state): State<WebState>,
    Json(options): Json<SyncOptions>,
) -> Response {
    let (job_id, _rx) = state.jobs.start(&state.store, options);
    let Some(snapshot) = state.jobs.snapshot(&job_id) else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {
                    "code": "job_not_found",
                    "message": "刚创建的 job 未能在 registry 中找到",
                }
            })),
        )
            .into_response();
    };
    Json(json!({
        "job_id": job_id,
        "snapshot": snapshot,
    }))
    .into_response()
}

async fn api_jobs_get(State(state): State<WebState>, Path(id): Path<String>) -> Response {
    match state.jobs.snapshot(&id) {
        Some(snapshot) => Json(json!(snapshot)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "job_not_found",
                    "message": format!("未知 job: {id}"),
                }
            })),
        )
            .into_response(),
    }
}

async fn api_jobs_cancel(State(state): State<WebState>, Path(id): Path<String>) -> Response {
    if !state.jobs.cancel(&id) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "job_not_found",
                    "message": format!("未知 job: {id}"),
                }
            })),
        )
            .into_response();
    }
    Json(json!({
        "ok": true,
        "snapshot": state.jobs.snapshot(&id),
    }))
    .into_response()
}

async fn load_via_dashboard<T, F>(state: WebState, f: F) -> LlmusageResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
{
    load_via_dashboard_with_timeout(state, WEB_API_TIMEOUT, f).await
}

async fn load_via_dashboard_with_timeout<T, F>(
    state: WebState,
    timeout: Duration,
    f: F,
) -> LlmusageResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
{
    with_timeout(
        timeout,
        tokio::task::spawn_blocking(move || {
            let dashboard = Dashboard::open(&state.store)?;
            f(&dashboard)
        }),
    )
    .await
}

async fn load_behavior_api<T, F, D>(state: WebState, f: F, degraded: D) -> LlmusageResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
    D: FnOnce(String) -> T,
{
    match load_via_dashboard_with_timeout(state, WEB_BEHAVIOR_API_TIMEOUT, f).await {
        Ok(value) => Ok(value),
        Err(err) => Ok(degraded(err.to_string())),
    }
}

async fn with_timeout<T>(
    duration: Duration,
    task: tokio::task::JoinHandle<LlmusageResult<T>>,
) -> LlmusageResult<T>
where
    T: Send + 'static,
{
    match tokio::time::timeout(duration, task).await {
        Ok(joined) => joined.map_err(|err| LlmusageError::ConfigInvalid {
            detail: format!("blocking dashboard task failed: {err}"),
        })?,
        Err(_) => Err(LlmusageError::ConfigInvalid {
            detail: format!(
                "dashboard query exceeded {} ms timeout",
                duration.as_millis()
            ),
        }),
    }
}

async fn load_dashboard_snapshot_resilient(
    state: WebState,
    filter: QueryFilter,
) -> LlmusageResult<serde_json::Value> {
    let core = load_via_dashboard(state.clone(), {
        let filter = filter.clone();
        move |dashboard| dashboard.core_snapshot(&filter)
    })
    .await?;

    let activity_filter = filter.clone();
    let activity = load_behavior_api(
        state.clone(),
        move |dashboard| dashboard.activity_breakdown(&activity_filter),
        degraded_activity,
    );
    let tools_filter = filter.clone();
    let tools = load_behavior_api(
        state.clone(),
        move |dashboard| dashboard.tool_breakdown(&tools_filter),
        degraded_tools,
    );
    let optimize_filter = filter.clone();
    let optimize = load_behavior_api(
        state.clone(),
        move |dashboard| dashboard.optimize(&optimize_filter),
        degraded_optimize,
    );
    let compare = load_behavior_api(
        state,
        move |dashboard| dashboard.model_compare(&filter, None, None),
        degraded_compare,
    );
    let (activity, tools, optimize, compare) = tokio::join!(activity, tools, optimize, compare);
    let activity = activity?;
    let tools = tools?;
    let optimize = optimize?;
    let compare = compare?;

    Ok(json!({
        "overview": core.overview,
        "day_trends": core.day_trends,
        "week_trends": core.week_trends,
        "month_trends": core.month_trends,
        "all_trends": core.all_trends,
        "models": core.models,
        "sources": core.sources,
        "projects": core.projects,
        "costs": core.costs,
        "activity": activity,
        "tools": tools,
        "optimize": optimize,
        "compare": compare,
        "health": core.health,
        "diagnostics": core.diagnostics,
    }))
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

fn dashboard_filter_from_params(params: &HashMap<String, String>) -> QueryFilter {
    let mut filter = dashboard_filter_from_params_without_window(params);

    if filter.since.is_none() && filter.until.is_none() {
        let range = params
            .get("range")
            .or_else(|| params.get("window"))
            .map(String::as_str);
        apply_window_filter(range, &mut filter);
    }

    filter
}

fn dashboard_filter_from_params_without_window(params: &HashMap<String, String>) -> QueryFilter {
    QueryFilter {
        source: params
            .get("source")
            .and_then(|raw| SourceKind::parse_id(raw.trim())),
        model: query_string(params, "model"),
        since: query_date(params, "since"),
        until: query_date(params, "until"),
        project_hash: query_string(params, "project_hash")
            .or_else(|| query_string(params, "project")),
        timezone: query_timezone(params.get("timezone").or_else(|| params.get("tz"))),
    }
}

fn query_string(params: &HashMap<String, String>, key: &str) -> Option<String> {
    params
        .get(key)
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty() && !raw.eq_ignore_ascii_case("all"))
}

fn query_date(params: &HashMap<String, String>, key: &str) -> Option<NaiveDate> {
    params
        .get(key)
        .and_then(|raw| NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok())
}

fn query_timezone(value: Option<&String>) -> crate::query::ReportTimezone {
    let Some(raw) = value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return crate::query::ReportTimezone::Local;
    };
    if raw.eq_ignore_ascii_case("utc") || raw == "Z" {
        return crate::query::ReportTimezone::Utc;
    }
    if raw.eq_ignore_ascii_case("local") {
        return crate::query::ReportTimezone::Local;
    }
    parse_fixed_offset(raw)
        .map(crate::query::ReportTimezone::Fixed)
        .unwrap_or(crate::query::ReportTimezone::Local)
}

fn parse_fixed_offset(raw: &str) -> Option<FixedOffset> {
    let normalized = raw
        .strip_prefix("UTC")
        .or_else(|| raw.strip_prefix("utc"))
        .unwrap_or(raw);
    let sign = normalized.chars().next()?;
    if !matches!(sign, '+' | '-') {
        return None;
    }
    let rest = &normalized[sign.len_utf8()..];
    let (hours, minutes) = if let Some((hours, minutes)) = rest.split_once(':') {
        (hours.parse::<i32>().ok()?, minutes.parse::<i32>().ok()?)
    } else if rest.len() == 4 {
        (
            rest[..2].parse::<i32>().ok()?,
            rest[2..].parse::<i32>().ok()?,
        )
    } else {
        (rest.parse::<i32>().ok()?, 0)
    };
    if hours > 23 || minutes > 59 {
        return None;
    }
    let seconds = hours * 3600 + minutes * 60;
    if sign == '-' {
        FixedOffset::west_opt(seconds)
    } else {
        FixedOffset::east_opt(seconds)
    }
}

fn apply_window_filter(window: Option<&str>, filter: &mut QueryFilter) {
    let Some(window) = window.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let today = chrono::Local::now().date_naive();
    match window {
        "today" => {
            filter.since = Some(today);
            filter.until = Some(today);
        }
        "day" | "24h" | "1d" => {
            filter.since = today.pred_opt();
            filter.until = Some(today);
        }
        "week" | "7d" => {
            filter.since = today.checked_sub_days(chrono::Days::new(6));
            filter.until = Some(today);
        }
        "month" | "30d" => {
            filter.since = today.checked_sub_days(chrono::Days::new(29));
            filter.until = Some(today);
        }
        "all" => {}
        _ => {}
    }
}

async fn api_json_async<T, Fut>(endpoint: &'static str, result: Fut) -> Response
where
    T: Serialize,
    Fut: Future<Output = LlmusageResult<T>>,
{
    api_json(endpoint, result.await)
}

fn api_json<T>(endpoint: &'static str, result: LlmusageResult<T>) -> Response
where
    T: Serialize,
{
    match result {
        Ok(value) => Json(json!(value)).into_response(),
        Err(err) => {
            error!(endpoint, error = %err, "Web API handler failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "code": "internal_error",
                        "message": "读取本地数据失败",
                        "detail": err.to_string(),
                        "endpoint": endpoint,
                    }
                })),
            )
                .into_response()
        }
    }
}

fn parse_bool_query(value: Option<&String>) -> bool {
    value
        .map(|raw| raw.trim().to_ascii_lowercase())
        .is_some_and(|raw| matches!(raw.as_str(), "1" | "true" | "yes" | "on"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::SocketAddr,
        time::{Duration, Instant},
    };

    use axum::{body::to_bytes, http::StatusCode};
    use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
    use tempfile::TempDir;

    use crate::{AppPaths, LlmusageError, store::Store, sync::SyncOptions};

    use super::{api_json, asset_manifest, live_index_html, serve, snapshot_index_html};

    fn make_store() -> anyhow::Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    async fn route_json(
        addr: SocketAddr,
        method: &str,
        path: &str,
        body: Option<String>,
    ) -> anyhow::Result<(StatusCode, serde_json::Value)> {
        let method = method.to_string();
        let path = path.to_string();
        let raw = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr)?;
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            let body = body.unwrap_or_default();
            let request = format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 Content-Type: application/json\r\n\
                 Accept: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\r\n\
                 {body}",
                body.len()
            );
            stream.write_all(request.as_bytes())?;
            let mut raw = String::new();
            stream.read_to_string(&mut raw)?;
            Ok::<_, anyhow::Error>(raw)
        })
        .await??;

        let (head, body) = raw
            .split_once("\r\n\r\n")
            .ok_or_else(|| anyhow::anyhow!("invalid HTTP response: {raw:?}"))?;
        let status_code = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|raw| raw.parse::<u16>().ok())
            .ok_or_else(|| anyhow::anyhow!("invalid status line: {head:?}"))?;
        let body = decode_http_body(head, body)?;
        let payload = serde_json::from_str(&body)?;
        Ok((StatusCode::from_u16(status_code)?, payload))
    }

    async fn route_text(
        addr: SocketAddr,
        method: &str,
        path: &str,
    ) -> anyhow::Result<(StatusCode, String)> {
        let method = method.to_string();
        let path = path.to_string();
        let raw = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr)?;
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            let request = format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 Accept: */*\r\n\
                 Content-Length: 0\r\n\
                 Connection: close\r\n\r\n"
            );
            stream.write_all(request.as_bytes())?;
            let mut raw = String::new();
            stream.read_to_string(&mut raw)?;
            Ok::<_, anyhow::Error>(raw)
        })
        .await??;

        let (head, body) = raw
            .split_once("\r\n\r\n")
            .ok_or_else(|| anyhow::anyhow!("invalid HTTP response: {raw:?}"))?;
        let status_code = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|raw| raw.parse::<u16>().ok())
            .ok_or_else(|| anyhow::anyhow!("invalid status line: {head:?}"))?;
        Ok((
            StatusCode::from_u16(status_code)?,
            decode_http_body(head, body)?,
        ))
    }

    fn decode_http_body(head: &str, body: &str) -> anyhow::Result<String> {
        if !head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            return Ok(body.to_string());
        }
        let mut rest = body;
        let mut decoded = String::new();
        while let Some((size_hex, after_size)) = rest.split_once("\r\n") {
            let size = usize::from_str_radix(size_hex.trim(), 16)?;
            if size == 0 {
                break;
            }
            if after_size.len() < size + 2 {
                return Err(anyhow::anyhow!("truncated chunked response"));
            }
            decoded.push_str(&after_size[..size]);
            rest = &after_size[size + 2..];
        }
        Ok(decoded)
    }

    #[test]
    fn live_shell_uses_module_entry() {
        let html = live_index_html();
        assert!(html.contains("data-mode=\"live\""));
        assert!(html.contains("data-app-version=\""));
        assert!(html.contains("data-supported-sources=\"codex, claude, opencode, gemini\""));
        assert!(html.contains("type=\"module\""));
        assert!(html.contains("assets/app.js"));
        assert!(html.contains("assets/base.css"));
        assert!(html.contains("assets/layout.css"));
        assert!(html.contains("assets/components.css"));
        assert!(html.contains("assets/charts.css"));
        assert!(html.contains("rel=\"icon\""));
        assert!(html.contains("assets/favicon.svg"));
        assert!(html.contains("aria-label=\"llmusage\""));
        assert!(!html.contains("<div class=\"brand-mark\">l</div>"));
        assert!(html.contains("本地用量<span class=\"accent\">概览</span>"));
        assert!(html.contains("id=\"overview\""));
        assert!(html.contains("id=\"trends\""));
        assert!(html.contains("id=\"models\""));
        assert!(html.contains("id=\"sources\""));
        assert!(html.contains("id=\"projects\""));
        assert!(html.contains("id=\"behavior\""));
        assert!(html.contains("id=\"activity-table\""));
        assert!(html.contains("id=\"tools-table\""));
        assert!(html.contains("id=\"optimize-summary\""));
        assert!(html.contains("id=\"optimize-findings\""));
        assert!(html.contains("id=\"compare-panel\""));
        assert!(html.contains("id=\"cost\""));
        assert!(html.contains("id=\"status\""));
        assert!(html.contains("id=\"insights-card\""));
        assert!(html.contains("id=\"filter-rail\""));
        assert!(html.contains("data-filter=\"source\""));
        assert!(html.contains("data-filter=\"model\""));
        assert!(html.contains("data-range-preset=\"1d\""));
        assert!(html.contains("data-range-preset=\"7d\""));
        assert!(html.contains("data-range-preset=\"30d\""));
        assert!(html.contains("data-date-input type=\"text\""));
        assert!(!html.contains("type=\"date\""));
        assert!(html.contains("id=\"auto-refresh\""));
        assert!(html.contains("data-refresh-interval=\"30000\""));
        assert!(html.contains("data-refresh-interval=\"60000\""));
    }

    #[test]
    fn dashboard_shell_uses_runtime_metadata_and_real_toggle_controls() {
        let html = live_index_html();
        assert!(html.contains(&format!("v{} · local", env!("CARGO_PKG_VERSION"))));
        assert!(html.contains(&format!(
            "llmusage v{} · local build",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(html.contains("data-toggle-panel=\"models\""));
        assert!(html.contains("data-toggle-panel=\"projects\""));
        assert!(html.contains("data-toggle-panel=\"costs\""));
        assert!(html.contains("aria-expanded=\"false\""));
        assert!(!html.contains(&["v0.", "4.2"].concat()));
        assert!(!html.contains(&["2026", ".05", ".06"].concat()));
    }

    #[test]
    fn snapshot_shell_uses_snapshot_mode_marker() {
        let html = snapshot_index_html();
        assert!(html.contains("data-mode=\"snapshot\""));
        assert!(html.contains("离线文件"));
        assert!(html.contains("type=\"module\""));
        assert!(html.contains("assets/app.js"));
    }

    #[test]
    fn asset_manifest_contains_required_files() {
        let paths = asset_manifest()
            .iter()
            .map(|asset| asset.path)
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "base.css",
                "layout.css",
                "components.css",
                "charts.css",
                "app.js",
                "copy.js",
                "i18n.js",
                "theme.js",
                "runtime.js",
                "data.js",
                "data/fetch.js",
                "data/format.js",
                "data/derive.js",
                "render.js",
                "render/hero.js",
                "render/trends.js",
                "render/models.js",
                "render/sources.js",
                "render/projects.js",
                "render/behavior.js",
                "render/costs.js",
                "render/insights.js",
                "render/charts.js",
                "render/tables.js",
                "render/health.js",
                "favicon.svg",
            ]
        );
    }

    #[test]
    fn render_assets_use_updated_terms() {
        let selected_bodies = asset_manifest()
            .iter()
            .filter(|asset| {
                matches!(
                    asset.path,
                    "copy.js"
                        | "render/hero.js"
                        | "render/charts.js"
                        | "render/tables.js"
                        | "render/health.js"
                )
            })
            .map(|asset| asset.body)
            .collect::<Vec<_>>()
            .join("\n");

        for old_term in [
            "账本摘要",
            "累计账本",
            "来源热度",
            "趋势聚焦",
            "模型偏好",
            "来源节奏",
            "项目热区",
            "运行脉冲",
            "明细账本",
        ] {
            assert!(
                !selected_bodies.contains(old_term),
                "found outdated term: {old_term}"
            );
        }
    }

    #[test]
    fn error_renderer_uses_text_content() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("renderHero(context)"));
        assert!(app_js.contains("renderTrends(context)"));
        assert!(app_js.contains("setupNavigation()"));
    }

    #[test]
    fn dashboard_assets_remove_demo_metrics_and_stale_metadata() {
        let selected_bodies = asset_manifest()
            .iter()
            .filter(|asset| {
                matches!(
                    asset.path,
                    "app.js" | "copy.js" | "data/derive.js" | "render/hero.js"
                )
            })
            .map(|asset| asset.body)
            .collect::<Vec<_>>()
            .join("\n");

        for stale in [
            ["+$3", ".4K"].concat(),
            ["$", "4", ",", "400"].concat(),
            format!("${}", "182.40"),
            ["~$", "28K"].concat(),
            ["v0.", "4.2"].concat(),
            ["2026", ".05", ".06"].concat(),
            ["source", "Suffix"].concat(),
            ["codex", " / ", "claude"].concat(),
            ["projects", "-anchor"].concat(),
        ] {
            assert!(
                !selected_bodies.contains(&stale),
                "found stale dashboard literal: {stale}"
            );
        }

        assert!(selected_bodies.contains("cache_savings_usd"));
        assert!(selected_bodies.contains("supportedSources"));
    }

    #[test]
    fn app_entry_wires_real_panel_toggles_and_project_navigation() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("setupPanelToggles(state)"));
        assert!(app_js.contains("state.expanded[panel] = !state.expanded[panel]"));
        assert!(app_js.contains("syncPanelToggleControls(context, dashboardState)"));
        assert!(app_js.contains("document.getElementById('projects')?.scrollIntoView"));
        assert!(!app_js.contains(&["projects", "-anchor"].concat()));
    }

    #[test]
    fn fetch_layer_reads_structured_error_payloads() {
        let fetch_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/fetch.js")
            .expect("fetch.js asset")
            .body;
        assert!(fetch_js.contains("payload?.error?.detail"));
        assert!(fetch_js.contains("response.clone().json()"));
    }

    #[test]
    fn app_entry_loads_dashboard_sections_instead_of_missing_window_global() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("loadDashboardSnapshot(state)"));
        assert!(app_js.contains("syncUrlFromState(state)"));
        assert!(app_js.contains("state.filters = currentFilterInputs()"));
        assert!(app_js.contains("setupRangePresetControls(state)"));
        assert!(app_js.contains("setupDateInputs(state)"));
        assert!(app_js.contains("date-picker-popover"));
        assert!(!app_js.contains("window.LLMUSAGE_DATA"));
    }

    #[test]
    fn fetch_layer_prefers_dashboard_snapshot_with_legacy_fallback_helpers() {
        let fetch_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/fetch.js")
            .expect("fetch.js asset")
            .body;
        assert!(fetch_js.contains("export async function loadDashboardSnapshot"));
        assert!(fetch_js.contains("loadJson(`/api/dashboard${buildFilterQuery(state)}`)"));
        assert!(fetch_js.contains("export async function loadSection"));
        assert!(fetch_js.contains("state?.rangePreset"));
        assert!(fetch_js.contains("params.set('range', state.rangePreset)"));
        assert!(fetch_js.contains("回退到旧分段 API"));
        assert!(fetch_js.contains("snapshot.json"));
    }

    #[test]
    fn live_shell_includes_theme_and_locale_toggles() {
        let html = live_index_html();
        assert!(html.contains("id=\"toggle-theme\""));
        assert!(html.contains("id=\"toggle-locale\""));
        assert!(html.contains("class=\"sidebar-toggles\""));
    }

    #[test]
    fn live_shell_declares_default_theme_and_locale() {
        let html = live_index_html();
        assert!(html.contains("data-theme=\"light\""));
        assert!(html.contains("data-locale=\"zh\""));
        assert!(html.contains("data-i18n-title=\"shell.window.title\""));
    }

    #[test]
    fn live_shell_uses_data_i18n_for_chrome() {
        let html = live_index_html();
        for key in [
            "data-i18n=\"shell.nav.item.usage\"",
            "data-i18n=\"shell.nav.item.trend\"",
            "data-i18n=\"shell.nav.item.behavior\"",
            "data-i18n=\"shell.nav.item.cost\"",
            "data-i18n=\"shell.behavior.optimize.title\"",
            "data-i18n=\"shell.behavior.compare.title\"",
            "data-i18n=\"shell.btn.export\"",
            "data-i18n=\"shell.btn.sync\"",
            "data-i18n=\"shell.filters.range\"",
            "data-i18n=\"shell.filters.range.1d\"",
            "data-i18n=\"shell.filters.range.7d\"",
            "data-i18n=\"shell.filters.range.30d\"",
            "data-i18n=\"shell.filters.apply\"",
            "data-i18n=\"shell.filters.reset\"",
            "data-i18n=\"shell.endpoint.lastSync\"",
            "data-i18n=\"shell.crumb.local\"",
            "data-i18n=\"shell.tag.local\"",
            "data-i18n-html=\"shell.hero.title.html\"",
        ] {
            assert!(html.contains(key), "missing i18n key in shell HTML: {key}");
        }
        assert!(
            !html.contains("data-i18n=\"shell.brand.sub\""),
            "brand version is runtime metadata and must not be overwritten by static i18n"
        );
        assert!(
            !html.contains("data-i18n=\"shell.footer.build\""),
            "footer version is runtime metadata and must not be overwritten by static i18n"
        );
    }

    #[test]
    fn snapshot_shell_uses_snapshot_chip_key() {
        let html = snapshot_index_html();
        assert!(html.contains("data-i18n=\"shell.tag.snapshot\""));
        assert!(html.contains("离线文件"));
    }

    #[test]
    fn live_shell_inlines_theme_locale_bootstrap() {
        let html = live_index_html();
        assert!(html.contains("llmusage:theme"));
        assert!(html.contains("llmusage:locale"));
    }

    #[test]
    fn copy_module_exposes_locale_api() {
        let copy_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "copy.js")
            .expect("copy.js asset")
            .body;
        assert!(copy_js.contains("export let UI_COPY"));
        assert!(copy_js.contains("export function setLocale"));
        assert!(copy_js.contains("export function getLocale"));
        assert!(copy_js.contains("UI_COPY_EN"));
    }

    #[test]
    fn theme_module_exposes_setter() {
        let theme_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "theme.js")
            .expect("theme.js asset")
            .body;
        assert!(theme_js.contains("export function setTheme"));
        assert!(theme_js.contains("export function toggleTheme"));
        assert!(theme_js.contains("llmusage:theme"));
    }

    #[test]
    fn i18n_module_walks_data_attributes() {
        let i18n_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "i18n.js")
            .expect("i18n.js asset")
            .body;
        assert!(i18n_js.contains("[data-i18n]"));
        assert!(i18n_js.contains("[data-i18n-html]"));
        assert!(i18n_js.contains("[data-i18n-attr]"));
    }

    #[test]
    fn app_entry_wires_theme_locale_setup() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("setupThemeToggle"));
        assert!(app_js.contains("setupLocaleToggle"));
        assert!(app_js.contains("setupSyncJob(state)"));
        assert!(app_js.contains("setupAutoRefresh(state)"));
        assert!(app_js.contains("renderInsights(context)"));
        assert!(app_js.contains("initTheme()"));
        assert!(app_js.contains("applyDomI18n(document)"));
        assert!(app_js.contains("onLocaleChange"));
    }

    #[test]
    fn app_entry_wires_auto_refresh_controls() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        let copy_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "copy.js")
            .expect("copy.js asset")
            .body;

        assert!(app_js.contains("AUTO_REFRESH_STORAGE_KEY"));
        assert!(app_js.contains("window.setInterval"));
        assert!(app_js.contains("data-refresh-interval"));
        assert!(app_js.contains("document.hidden"));
        assert!(app_js.contains("state.activeJobSnapshot?.status === 'running'"));
        assert!(app_js.contains("shell.refresh.snapshotDisabled"));
        assert!(copy_js.contains("shell.refresh.label"));
        assert!(copy_js.contains("shell.refresh.failed"));
    }

    #[test]
    fn app_entry_wires_sync_job_lifecycle() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("postJson('/api/jobs', syncOptionsFromState(state))"));
        assert!(app_js.contains("/api/jobs/${encodeURIComponent(state.activeJobId)}/cancel"));
        assert!(app_js.contains("pollJobUntilTerminal(state, state.activeJobId)"));
        assert!(app_js.contains("await reloadDashboard(state)"));
        assert!(app_js.contains("shell.sync.snapshotDisabled"));
    }

    #[test]
    fn app_entry_binds_core_controls_before_dashboard_data_load() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        let before_load = app_js
            .split("state.rawData = await loadDashboardData(state);")
            .next()
            .expect("main load marker");
        for marker in [
            "setupNavigation()",
            "setupFilterControls(state)",
            "setupPanelToggles(state)",
            "setupSyncJob(state)",
            "setupThemeToggle()",
            "setupLocaleToggle(state)",
        ] {
            assert!(
                before_load.contains(marker),
                "{marker} must be bound before dashboard data can fail"
            );
        }
    }

    #[test]
    fn fetch_layer_degrades_behavior_sections_without_blocking_core() {
        let fetch_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/fetch.js")
            .expect("fetch.js asset")
            .body;
        assert!(fetch_js.contains("loadOptionalSection(state, 'activity'"));
        assert!(fetch_js.contains("loadOptionalSection(state, 'tools'"));
        assert!(fetch_js.contains("loadOptionalSection(state, 'optimize'"));
        assert!(fetch_js.contains("loadOptionalSection(state, 'compare'"));
        assert!(fetch_js.contains("level: 'degraded'"));
    }

    #[test]
    fn dashboard_assets_surface_diagnostics_insights_without_fake_claims() {
        let fetch_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/fetch.js")
            .expect("fetch.js asset")
            .body;
        let derive_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/derive.js")
            .expect("derive.js asset")
            .body;
        let insights_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/insights.js")
            .expect("insights.js asset")
            .body;
        let copy_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "copy.js")
            .expect("copy.js asset")
            .body;

        assert!(fetch_js.contains("diagnostics: snapshot?.diagnostics"));
        assert!(fetch_js.contains("loadSection(state, 'diagnostics', '/api/diagnostics')"));
        assert!(derive_js.contains("function buildInsights"));
        assert!(derive_js.contains("cache_efficiency"));
        assert!(derive_js.contains("pricing_status"));
        assert!(derive_js.contains("lossy_rebuild_risk"));
        assert!(derive_js.contains("普通 sync 不会删除已导入历史"));
        assert!(insights_js.contains("insight-note"));
        assert!(copy_js.contains("not final diagnoses"));
        assert!(copy_js.contains("不是最终诊断"));
    }

    #[test]
    fn trend_context_keeps_chart_chronological_and_table_recent_first() {
        let derive_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/derive.js")
            .expect("derive.js asset")
            .body;
        assert!(derive_js.contains("const chronologicalRows = normalizeTrendRows(trends);"));
        assert!(derive_js.contains("const recentRowsDesc = [...chronologicalRows].reverse();"));
        assert!(
            derive_js.contains(
                "const spotlightRows = recentRowsDesc\n    .slice(0, PANEL_LIMITS.trendSpotlight)\n    .reverse();"
            ),
            "spotlight chart rows must take the latest N records, then restore chronological order"
        );
        assert!(
            derive_js
                .contains("const tableRows = recentRowsDesc.slice(0, PANEL_LIMITS.trendTable);")
        );
        assert!(derive_js.contains("chronologicalRows,"));
        assert!(derive_js.contains("recentRowsDesc,"));
    }

    #[test]
    fn trend_renderer_uses_derived_spotlight_rows_without_reslicing_ledger() {
        let trends_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/trends.js")
            .expect("render/trends.js asset")
            .body;
        assert!(trends_js.contains("const spotlightRows = context.trend.spotlightRows || [];"));
        assert!(
            !trends_js.contains("trendLedgerRows.slice"),
            "render layer must not derive chart rows from ledger rows"
        );
        assert!(
            !trends_js.contains("ledgerRows.slice"),
            "render layer must not reslice latest-first ledger rows for the chart"
        );
        assert!(trends_js.contains("trend-empty-title"));
    }

    #[test]
    fn trend_chart_assets_expose_peak_and_empty_styles() {
        let charts_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "charts.css")
            .expect("charts.css asset")
            .body;
        assert!(charts_css.contains(".trend-bar.is-peak"));
        assert!(charts_css.contains(".trend-peak-label"));
        assert!(charts_css.contains(".trend-empty-title"));
        assert!(charts_css.contains("min-width: 560px"));
    }

    #[test]
    fn api_error_payload_is_structured_json() {
        let response =
            api_json::<serde_json::Value>("/api/test", Err(LlmusageError::NotInitialized));
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let body = runtime.block_on(async {
            to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body bytes")
        });
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json payload");
        assert_eq!(payload["error"]["code"], "internal_error");
        assert_eq!(payload["error"]["endpoint"], "/api/test");
        assert!(
            payload["error"]["detail"]
                .as_str()
                .unwrap()
                .contains("llmusage init")
        );
    }

    #[tokio::test]
    async fn api_dashboard_returns_core_snapshot_when_behavior_sections_are_degraded()
    -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            )
            VALUES ('codex', 'gpt-5', '2026-05-01T00:00:00Z', 'project-a', 'Project A', NULL,
                    10, 0, 0, 0, 0, 10, 0.1, 0.1, 'static', 'static-v1',
                    1, '2026-05-01T00:00:00Z')
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
            ) VALUES ('broken-turn', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                1, 0, 1, 1, 10, 0, 0, 0, 0, 10, '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        conn.execute("DROP TABLE usage_turn", [])?;
        conn.execute(
            "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
            [],
        )?;
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(addr, "GET", "/api/dashboard", None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["overview"]["total"]["total_tokens"], 10);
        assert_eq!(payload["activity"]["support"]["level"], "degraded");
        assert!(
            payload["activity"]["breakdown"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(payload["models"].as_array().unwrap().len() == 1);
        Ok(())
    }

    #[tokio::test]
    async fn api_logs_rejects_malformed_cursor() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;
        let (status, payload) =
            route_json(addr, "GET", "/api/logs?cursor=not-base64-json", None).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(payload["error"]["code"], "invalid_cursor");
        Ok(())
    }

    #[tokio::test]
    async fn api_trends_daily_exposes_daily_cost_series() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            )
            VALUES ('codex', 'gpt-5', '2026-05-01T00:00:00Z', '', NULL, NULL,
                    10, 2, 0, 5, 1, 18, 0.25, 0.30, 'static', 'static-v1',
                    1, '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(addr, "GET", "/api/trends_daily", None).await?;
        assert_eq!(status, StatusCode::OK);
        let first = payload
            .as_array()
            .and_then(|rows| rows.first())
            .expect("trend row");
        assert_eq!(first["date"], "2026-05-01");
        assert_eq!(first["event_count"], 1);
        assert_eq!(first["cost_with_cache_usd"], 0.25);
        Ok(())
    }

    #[tokio::test]
    async fn api_trends_day_returns_labels_ascending_from_unordered_buckets() -> anyhow::Result<()>
    {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        let base = Utc::now() - ChronoDuration::hours(3);
        let expected_labels = [0, 1, 2].map(|offset| {
            (base + ChronoDuration::hours(offset)).to_rfc3339_opts(SecondsFormat::Secs, true)
        });
        for (hour_start, total_tokens) in [
            (&expected_labels[2], 30),
            (&expected_labels[0], 10),
            (&expected_labels[1], 20),
        ] {
            conn.execute(
                r#"
                INSERT INTO usage_bucket_30m(
                    source, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    event_count, updated_at
                )
                VALUES ('codex', 'gpt-5', ?1, '', NULL, NULL,
                        ?2, 0, 0, 0, 0, ?2, 1, ?1)
                "#,
                rusqlite::params![hour_start, total_tokens],
            )?;
        }
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(addr, "GET", "/api/trends?window=day", None).await?;
        assert_eq!(status, StatusCode::OK);
        let rows = payload.as_array().expect("trend array");
        let actual_labels = rows
            .iter()
            .map(|row| row["label"].as_str().expect("label").to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            actual_labels,
            vec![
                expected_labels[0].clone(),
                expected_labels[1].clone(),
                expected_labels[2].clone()
            ]
        );
        Ok(())
    }

    #[tokio::test]
    async fn dashboard_apis_share_filter_query_parameters() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        for (source, model, hour_start, tokens, cost, project_hash) in [
            (
                "codex",
                "gpt-5",
                "2026-05-01T00:00:00Z",
                10,
                0.10,
                "project-a",
            ),
            (
                "claude",
                "sonnet",
                "2026-05-01T00:00:00Z",
                20,
                0.20,
                "project-b",
            ),
            (
                "codex",
                "gpt-5",
                "2026-05-03T00:00:00Z",
                30,
                0.30,
                "project-a",
            ),
        ] {
            conn.execute(
                r#"
                INSERT INTO usage_bucket_30m(
                    source, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                    event_count, updated_at
                )
                VALUES (?1, ?2, ?3, ?6, ?6, NULL,
                        ?4, 0, 0, 0, 0, ?4,
                        ?5, ?5, 'static', 'static-v1',
                        1, ?3)
                "#,
                rusqlite::params![source, model, hour_start, tokens, cost, project_hash],
            )?;
        }
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let filter = "source=codex&model=gpt-5&since=2026-05-02&until=2026-05-03&timezone=UTC";

        let (status, overview) =
            route_json(addr, "GET", &format!("/api/overview?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(overview["total"]["total_tokens"], 30);
        assert_eq!(overview["total_events"], 1);
        assert_eq!(overview["total_cost_usd"], 0.30);

        let (status, models) =
            route_json(addr, "GET", &format!("/api/models?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        let models = models.as_array().expect("models array");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0]["model"], "gpt-5");
        assert_eq!(models[0]["total_tokens"], 30);

        let (status, projects) =
            route_json(addr, "GET", &format!("/api/projects?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        let projects = projects.as_array().expect("projects array");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["project_hash"], "project-a");

        let (status, costs) =
            route_json(addr, "GET", &format!("/api/costs?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        let costs = costs.as_array().expect("costs array");
        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0]["source"], "codex");
        assert_eq!(costs[0]["estimated_cost_usd"], 0.30);
        Ok(())
    }

    #[tokio::test]
    async fn behavior_apis_return_activity_tools_and_snapshot_fields() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_creation_tokens, cache_read_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate
            )
            VALUES ('codex:behavior:web', 'codex', 'gpt-5',
                    '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
                    100, 0, 0, 50, 0, 150,
                    'project-a', 'Project A', NULL, 'path-a',
                    'session-a', NULL, 'path-a', '2026-05-01T00:00:00Z',
                    0.5, 0.5, 'static', 'static-v1', NULL)
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            )
            VALUES ('codex', 'gpt-5', '2026-05-01T00:00:00Z', 'project-a', 'Project A', NULL,
                    100, 0, 0, 50, 0, 150, 0.5, 0.5, 'static', 'static-v1',
                    1, '2026-05-01T00:00:00Z')
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
            ) VALUES ('turn:codex:behavior:web', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                1, 0, 1, 1, 100, 0, 0, 50, 0, 150, '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES ('tool:codex:behavior:web', 'turn:codex:behavior:web',
                'codex:behavior:web', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T00:00:00Z', 'Edit', 'edit',
                NULL, NULL, 'fp-edit', 'Edit src/web/mod.rs', '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let filter = "source=codex&model=gpt-5&timezone=UTC";

        let (status, activity) =
            route_json(addr, "GET", &format!("/api/activity?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(activity["support"]["supported"], true);
        assert_eq!(activity["breakdown"][0]["category"], "coding");
        assert_eq!(activity["breakdown"][0]["one_shot_rate"], 1.0);

        let (status, tools) =
            route_json(addr, "GET", &format!("/api/tools?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(tools["support"]["supported"], true);
        assert_eq!(tools["breakdown"][0]["tool_kind"], "edit");
        assert_eq!(tools["breakdown"][0]["call_share"], 1.0);

        let (status, dashboard) =
            route_json(addr, "GET", &format!("/api/dashboard?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(dashboard["activity"]["breakdown"][0]["category"], "coding");
        assert_eq!(dashboard["tools"]["breakdown"][0]["tool_name"], "Edit");
        Ok(())
    }

    #[tokio::test]
    async fn static_assets_respond_while_behavior_api_is_scanning_large_table() -> anyhow::Result<()>
    {
        let (_temp, store) = make_store()?;
        {
            let conn = store.open_connection()?;
            let tx = conn.unchecked_transaction()?;
            for idx in 0..5_000 {
                tx.execute(
                    r#"
                    INSERT INTO usage_turn(
                        turn_key, source, session_id, source_path_hash, project_hash,
                        primary_model, started_at, category, has_edits, retries,
                        one_shot, call_count, input_tokens, cache_read_tokens,
                        cache_creation_tokens, output_tokens, reasoning_output_tokens,
                        total_tokens, created_at
                    ) VALUES (?1, 'codex', 'session-large', 'path-large',
                        'project-large', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                        1, 0, 1, 1, 10, 0, 0, 5, 0, 15, '2026-05-01T00:00:00Z')
                    "#,
                    [format!("turn:large:{idx}")],
                )?;
            }
            tx.commit()?;
        }
        let addr = serve(store, Some(0)).await?;

        let api_addr = addr;
        let activity = tokio::spawn(async move {
            route_json(api_addr, "GET", "/api/activity?source=codex", None).await
        });
        let started = Instant::now();
        let (asset_status, asset_body) = route_text(addr, "GET", "/assets/app.js").await?;
        assert_eq!(asset_status, StatusCode::OK);
        assert!(asset_body.contains("llmusage dashboard"));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "asset response should not wait behind behavior query"
        );
        let (activity_status, payload) = activity.await??;
        assert_eq!(activity_status, StatusCode::OK);
        assert!(payload["support"]["supported"].as_bool().unwrap_or(false));
        Ok(())
    }

    #[tokio::test]
    async fn optimize_and_compare_apis_return_explicit_behavior_payloads() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        for (index, model, event_key, turn_key, tool_kind, tool_name, preview) in [
            (
                0,
                "gpt-5",
                "codex:behavior:compare:a",
                "turn:codex:behavior:compare:a",
                "edit",
                "Edit",
                "Edit src/lib.rs",
            ),
            (
                1,
                "sonnet",
                "codex:behavior:compare:b",
                "turn:codex:behavior:compare:b",
                "read",
                "Read",
                "Read node_modules/pkg/index.js",
            ),
            (
                2,
                "sonnet",
                "codex:behavior:compare:c",
                "turn:codex:behavior:compare:c",
                "read",
                "Read",
                "Read node_modules/pkg/index.js",
            ),
        ] {
            conn.execute(
                r#"
                INSERT INTO usage_event(
                    event_key, source, model, event_at, hour_start,
                    input_tokens, cache_creation_tokens, cache_read_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    project_hash, project_label, project_ref, path_hash,
                    session_id, session_label, source_path_hash, created_at,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate
                )
                VALUES (?1, 'codex', ?2,
                        '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
                        100, 0, 10, 50, 0, 160,
                        'project-a', 'Project A', NULL, 'path-a',
                        'session-a', NULL, 'path-a', '2026-05-01T00:00:00Z',
                        0.2, 0.2, 'static', 'static-v1', NULL)
                "#,
                rusqlite::params![event_key, model],
            )?;
            conn.execute(
                r#"
                INSERT INTO usage_bucket_30m(
                    source, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                    event_count, updated_at
                )
                VALUES ('codex', ?1, '2026-05-01T00:00:00Z', 'project-a', 'Project A', NULL,
                        100, 10, 0, 50, 0, 160, 0.2, 0.2, 'static', 'static-v1',
                        1, '2026-05-01T00:00:00Z')
                ON CONFLICT(source, model, hour_start, project_hash) DO UPDATE SET
                    input_tokens = input_tokens + excluded.input_tokens,
                    cache_read_tokens = cache_read_tokens + excluded.cache_read_tokens,
                    output_tokens = output_tokens + excluded.output_tokens,
                    total_tokens = total_tokens + excluded.total_tokens,
                    cost_with_cache_usd = cost_with_cache_usd + excluded.cost_with_cache_usd,
                    cost_without_cache_usd = cost_without_cache_usd + excluded.cost_without_cache_usd,
                    event_count = event_count + excluded.event_count,
                    updated_at = excluded.updated_at
                "#,
                [model],
            )?;
            conn.execute(
                r#"
                INSERT INTO usage_turn(
                    turn_key, source, session_id, source_path_hash, project_hash,
                    primary_model, started_at, category, has_edits, retries,
                    one_shot, call_count, input_tokens, cache_read_tokens,
                    cache_creation_tokens, output_tokens, reasoning_output_tokens,
                    total_tokens, created_at
                ) VALUES (?1, 'codex', 'session-a', 'path-a',
                    'project-a', ?2, '2026-05-01T00:00:00Z', 'coding',
                    ?3, ?4, ?5, 1, 100, 10, 0, 50, 0, 160, '2026-05-01T00:00:00Z')
                "#,
                rusqlite::params![
                    turn_key,
                    model,
                    i64::from(index == 0),
                    i64::from(index == 1),
                    i64::from(index == 0)
                ],
            )?;
            conn.execute(
                r#"
                INSERT INTO usage_tool_call(
                    tool_call_key, turn_key, event_key, source, session_id,
                    source_path_hash, project_hash, model, occurred_at, tool_name,
                    tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
                ) VALUES (?1, ?2, ?3, 'codex', 'session-a', 'path-a',
                    'project-a', ?4, '2026-05-01T00:00:00Z', ?5, ?6,
                    NULL, NULL, 'fp-shared', ?7, '2026-05-01T00:00:00Z')
                "#,
                rusqlite::params![
                    format!("tool:behavior:compare:{index}"),
                    turn_key,
                    event_key,
                    model,
                    tool_name,
                    tool_kind,
                    preview
                ],
            )?;
        }
        drop(conn);

        let addr = serve(store, Some(0)).await?;

        let (status, optimize) =
            route_json(addr, "GET", "/api/optimize?source=codex&timezone=UTC", None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(optimize["support"]["supported"], true);
        assert!(
            optimize["findings"]
                .as_array()
                .expect("findings")
                .iter()
                .any(|finding| finding["id"] == "duplicate_reads" || finding["id"] == "junk_reads")
        );

        let (status, candidates) = route_json(
            addr,
            "GET",
            "/api/compare/models?source=codex&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(candidates.as_array().expect("candidates").len(), 2);

        let (status, compare) = route_json(
            addr,
            "GET",
            "/api/compare?source=codex&model_a=gpt-5&model_b=sonnet&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(compare["support"]["level"], "low_sample");
        assert!(
            compare["metrics"]
                .as_array()
                .expect("metrics")
                .iter()
                .any(|metric| metric["id"] == "one_shot_rate")
        );
        Ok(())
    }

    #[tokio::test]
    async fn api_dashboard_returns_single_snapshot_with_filter() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        for (source, model, project_hash, project_label, tokens, cost) in [
            ("codex", "gpt-5", "project-a", "Project A", 10, 0.10),
            ("claude", "sonnet", "project-b", "Project B", 20, 0.20),
        ] {
            conn.execute(
                r#"
                INSERT INTO usage_bucket_30m(
                    source, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                    event_count, updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, NULL,
                        ?6, 0, 0, 0, 0, ?7,
                        ?8, ?9, 'static', 'static-v1',
                        1, ?3)
                "#,
                rusqlite::params![
                    source,
                    model,
                    "2026-05-01T00:00:00Z",
                    project_hash,
                    project_label,
                    tokens,
                    tokens,
                    cost,
                    cost
                ],
            )?;
        }
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(
            addr,
            "GET",
            "/api/dashboard?source=codex&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["overview"]["total"]["total_tokens"], 10);
        assert_eq!(payload["models"][0]["model"], "gpt-5");
        assert_eq!(payload["sources"][0]["source"], "codex");
        assert_eq!(payload["projects"][0]["project_hash"], "project-a");
        assert_eq!(payload["costs"][0]["source"], "codex");
        assert!(payload["health"].is_object());
        Ok(())
    }

    #[tokio::test]
    async fn api_dashboard_applies_window_to_snapshot_sections() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        let stale =
            (Utc::now() - ChronoDuration::days(20)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let fresh =
            (Utc::now() - ChronoDuration::hours(2)).to_rfc3339_opts(SecondsFormat::Secs, true);
        for (hour_start, tokens, model) in [(&stale, 10, "old-model"), (&fresh, 40, "fresh-model")]
        {
            conn.execute(
                r#"
                INSERT INTO usage_bucket_30m(
                    source, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                    event_count, updated_at
                )
                VALUES ('codex', ?3, ?1, ?3, ?3, NULL,
                        ?2, 0, 0, 0, 0, ?2,
                        ?2 * 0.01, ?2 * 0.01, 'static', 'static-v1',
                        1, ?1)
                "#,
                rusqlite::params![hour_start, tokens, model],
            )?;
        }
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(
            addr,
            "GET",
            "/api/dashboard?window=day&source=codex&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["overview"]["total"]["total_tokens"], 40);
        assert_eq!(payload["models"].as_array().expect("models").len(), 1);
        assert_eq!(payload["models"][0]["model"], "fresh-model");
        assert_eq!(payload["costs"][0]["model"], "fresh-model");

        let (status, payload) = route_json(
            addr,
            "GET",
            "/api/dashboard?window=all&range=1d&source=codex&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["overview"]["total"]["total_tokens"], 40);
        assert_eq!(payload["models"].as_array().expect("models").len(), 1);
        assert_eq!(payload["models"][0]["model"], "fresh-model");
        Ok(())
    }

    #[tokio::test]
    async fn api_dashboard_embeds_archive_diagnostics_for_insights() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO source_file(source, file_path, state, last_seen_at, last_state_change_at)
            VALUES ('codex', ?1, 'missing', NULL, '2026-05-01T00:00:00Z')
            "#,
            [r"D:\missing\codex.jsonl"],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_event(
                event_key, source, model, event_at, hour_start,
                input_tokens, cache_creation_tokens, cache_read_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                project_hash, project_label, project_ref, path_hash,
                session_id, session_label, source_path_hash, created_at,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source, pricing_rate
            )
            VALUES ('codex:event:diagnostics', 'codex', 'gpt-5',
                    '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
                    10, 0, 0, 0, 0, 10,
                    'project-a', 'Project A', NULL, 'path-a',
                    NULL, NULL, NULL, '2026-05-01T00:00:00Z',
                    0.1, 0.1, 'static', 'static-v1', NULL)
            "#,
            [],
        )?;
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(addr, "GET", "/api/dashboard", None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["diagnostics"]["by_source"][0]["source"], "codex");
        assert_eq!(
            payload["diagnostics"]["by_source"][0]["missing_file_count"],
            1
        );
        assert_eq!(
            payload["diagnostics"]["by_source"][0]["protected_event_count"],
            1
        );
        assert_eq!(
            payload["diagnostics"]["by_source"][0]["lossy_rebuild_risk"],
            true
        );
        Ok(())
    }

    #[tokio::test]
    async fn api_jobs_start_get_cancel_and_not_found() -> anyhow::Result<()> {
        let (temp, store) = make_store()?;
        let home = temp.path().join("home");
        let codex_home = temp.path().join("codex-home");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(codex_home.join("sessions"))?;
        let _env = EnvGuard::set([
            ("HOME", home.to_string_lossy().to_string()),
            ("USERPROFILE", home.to_string_lossy().to_string()),
            ("CODEX_HOME", codex_home.to_string_lossy().to_string()),
        ]);
        let addr = serve(store, Some(0)).await?;

        let body = serde_json::to_string(&SyncOptions {
            source: Some("codex".to_string()),
            ..Default::default()
        })?;
        let (status, payload) = route_json(addr, "POST", "/api/jobs", Some(body)).await?;
        assert_eq!(status, StatusCode::OK);
        let job_id = payload["job_id"]
            .as_str()
            .expect("job_id string")
            .to_string();
        assert_eq!(payload["snapshot"]["job_id"], job_id);
        assert!(payload["snapshot"]["status"].is_string());

        let (status, snapshot) =
            route_json(addr, "GET", &format!("/api/jobs/{job_id}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(snapshot["job_id"], job_id);

        let (status, cancelled) = route_json(
            addr,
            "POST",
            &format!("/api/jobs/{job_id}/cancel"),
            Some("{}".into()),
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(cancelled["ok"], true);
        assert_eq!(cancelled["snapshot"]["job_id"], job_id);

        let (status, missing_get) = route_json(addr, "GET", "/api/jobs/missing-job", None).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(missing_get["error"]["code"], "job_not_found");

        let (status, missing_cancel) = route_json(
            addr,
            "POST",
            "/api/jobs/missing-job/cancel",
            Some("{}".into()),
        )
        .await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(missing_cancel["error"]["code"], "job_not_found");
        Ok(())
    }

    struct EnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvGuard {
        fn set(values: impl IntoIterator<Item = (&'static str, String)>) -> Self {
            let mut saved = Vec::new();
            for (key, value) in values {
                saved.push((key, std::env::var_os(key)));
                unsafe { std::env::set_var(key, value) };
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..).rev() {
                unsafe {
                    if let Some(value) = value {
                        std::env::set_var(key, value);
                    } else {
                        std::env::remove_var(key);
                    }
                }
            }
        }
    }
}
