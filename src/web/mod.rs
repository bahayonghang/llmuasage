use std::{
    collections::HashMap,
    future::Future,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use chrono::{FixedOffset, NaiveDate};
use rusqlite::InterruptHandle;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_http::compression::CompressionLayer;
use tracing::{debug, error, info};

use crate::{
    error::{LlmusageError, Result as LlmusageResult},
    models::SourceKind,
    query::{
        ActivityPayload, BehaviorSupport, Dashboard, DiagnosticsPayload, ExplorerDimension,
        ExplorerFilters, ExplorerGranularity, ExplorerMetric, ExplorerQuery, ExplorerTokenType,
        LogsQuery, ModelComparePayload, OptimizePayload, QueryFilter, ToolsPayload,
    },
    store::Store,
    sync::{JobRegistry, SyncOptions},
};

const WEB_API_TIMEOUT: Duration = Duration::from_secs(5);
const WEB_BEHAVIOR_API_TIMEOUT: Duration = Duration::from_secs(1);
const WEB_DASHBOARD_QUERY_PERMITS: usize = 4;
/// Web/API read connections fail lock waits fast so section timeouts and
/// degraded fallbacks trigger inside the request budget; sync writers keep
/// the 30s default from `Store::open_connection`.
const WEB_READ_BUSY_TIMEOUT: Duration = Duration::from_millis(1_500);
/// Request-boundary cache TTL for `Dashboard::diagnostics()`.
///
/// Baseline (research/baseline.md): one cold diagnostics pass costs ~50ms on
/// a representative 2.7k-file database (~120ms on a 5k-file stress fixture)
/// and every dashboard load used to pay it. 30s keeps that cost amortized
/// while bounding how long an externally deleted file can go unnoticed; sync
/// job completion invalidates the entry immediately, which covers the main
/// freshness path. The query layer itself keeps cold-read semantics.
const DIAGNOSTICS_CACHE_TTL: Duration = Duration::from_secs(30);
const WEB_SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

mod assets;
mod brand;
mod shell;

/// Request-boundary cache for the `Dashboard::diagnostics()` payload.
///
/// Lives in `WebState` (the web request boundary) — never in the query
/// layer, which keeps cold-read semantics for `home_overview` and direct
/// `Dashboard::diagnostics()` callers. A single entry is shared by
/// `/api/diagnostics` and every dashboard scope; the tokio mutex provides
/// single-flight recomputation so a TTL expiry triggers exactly one cold
/// pass even under concurrent requests.
struct DiagnosticsCache {
    ttl: Duration,
    entry: std::sync::RwLock<Option<DiagnosticsCacheEntry>>,
    compute: tokio::sync::Mutex<()>,
    generation: AtomicU64,
}

struct DiagnosticsCacheEntry {
    payload: DiagnosticsPayload,
    computed_at: Instant,
}

impl DiagnosticsCache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entry: std::sync::RwLock::new(None),
            compute: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
        }
    }

    fn get_fresh(&self) -> Option<DiagnosticsPayload> {
        let guard = self.entry.read().ok()?;
        let entry = guard.as_ref()?;
        (entry.computed_at.elapsed() < self.ttl).then(|| entry.payload.clone())
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn store_if_generation(&self, payload: DiagnosticsPayload, expected_generation: u64) -> bool {
        if let Ok(mut guard) = self.entry.write() {
            if self.generation.load(Ordering::Acquire) != expected_generation {
                return false;
            }
            *guard = Some(DiagnosticsCacheEntry {
                payload,
                computed_at: Instant::now(),
            });
            return true;
        }
        false
    }

    fn invalidate(&self) {
        if let Ok(mut guard) = self.entry.write() {
            self.generation.fetch_add(1, Ordering::AcqRel);
            *guard = None;
        }
    }
}

#[derive(Clone)]
pub struct WebState {
    pub store: Store,
    pub jobs: JobRegistry,
    #[doc(hidden)]
    pub dashboard_query_semaphore: Arc<Semaphore>,
    diagnostics_cache: Arc<DiagnosticsCache>,
}

impl WebState {
    pub fn new(store: Store) -> Self {
        Self::with_jobs_and_query_limit(store, JobRegistry::default(), WEB_DASHBOARD_QUERY_PERMITS)
    }

    fn with_jobs_and_query_limit(store: Store, jobs: JobRegistry, permits: usize) -> Self {
        Self::with_diagnostics_cache_ttl(store, jobs, permits, DIAGNOSTICS_CACHE_TTL)
    }

    fn with_diagnostics_cache_ttl(
        store: Store,
        jobs: JobRegistry,
        permits: usize,
        diagnostics_cache_ttl: Duration,
    ) -> Self {
        let diagnostics_cache = Arc::new(DiagnosticsCache::new(diagnostics_cache_ttl));
        jobs.register_terminal_hook({
            let cache = Arc::downgrade(&diagnostics_cache);
            move || {
                if let Some(cache) = cache.upgrade() {
                    cache.invalidate();
                }
            }
        });
        Self {
            store,
            jobs,
            dashboard_query_semaphore: Arc::new(Semaphore::new(permits.max(1))),
            diagnostics_cache,
        }
    }
}

pub(crate) struct BoundWebServer {
    addr: SocketAddr,
    shutdown: CancellationToken,
    task: Option<JoinHandle<std::io::Result<()>>>,
}

impl BoundWebServer {
    pub(crate) fn addr(&self) -> SocketAddr {
        self.addr
    }

    #[cfg(test)]
    pub(crate) fn from_test_task(task: JoinHandle<std::io::Result<()>>) -> Self {
        Self {
            addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            shutdown: CancellationToken::new(),
            task: Some(task),
        }
    }

    pub(crate) async fn wait(&mut self) -> Result<()> {
        wait_for_server_task(
            self.task
                .as_mut()
                .context("Web server task is no longer owned")?,
        )
        .await
    }

    pub(crate) async fn shutdown(self) -> Result<()> {
        self.shutdown_with_timeout(WEB_SERVER_SHUTDOWN_TIMEOUT)
            .await
    }

    async fn shutdown_with_timeout(mut self, timeout: Duration) -> Result<()> {
        self.shutdown.cancel();
        let mut task = self
            .task
            .take()
            .context("Web server task is no longer owned")?;
        match tokio::time::timeout(timeout, &mut task).await {
            Ok(result) => server_task_result(result),
            Err(_) => {
                task.abort();
                let _ = task.await;
                bail!(
                    "Web server at {} did not stop within {} seconds",
                    self.addr,
                    timeout.as_secs_f64()
                );
            }
        }
    }

    pub(crate) fn detach_with_error_logging(mut self) -> SocketAddr {
        let addr = self.addr;
        if let Some(task) = self.task.take() {
            tokio::spawn(async move {
                if let Err(err) = server_task_result(task.await) {
                    error!(%addr, error = %err, "detached Web server stopped unexpectedly");
                }
            });
        }
        addr
    }
}

impl Drop for BoundWebServer {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            self.shutdown.cancel();
            task.abort();
        }
    }
}

async fn wait_for_server_task(task: &mut JoinHandle<std::io::Result<()>>) -> Result<()> {
    server_task_result(task.await)
}

fn server_task_result(
    result: std::result::Result<std::io::Result<()>, tokio::task::JoinError>,
) -> Result<()> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err).context("Web server task returned an error"),
        Err(err) if err.is_panic() => Err(err).context("Web server task panicked"),
        Err(err) => Err(err).context("Web server task was cancelled"),
    }
}

pub async fn serve(store: Store, preferred_port: Option<u16>) -> Result<SocketAddr> {
    serve_on(store, preferred_port, IpAddr::V4(Ipv4Addr::LOCALHOST)).await
}

pub(crate) async fn serve_on(
    store: Store,
    preferred_port: Option<u16>,
    bind_ip: IpAddr,
) -> Result<SocketAddr> {
    Ok(bind_server(store, preferred_port, bind_ip)
        .await?
        .detach_with_error_logging())
}

pub(crate) async fn bind_server(
    store: Store,
    preferred_port: Option<u16>,
    bind_ip: IpAddr,
) -> Result<BoundWebServer> {
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
    let state = WebState::new(store);
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
        .route("/api/explorer", get(api_explorer))
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
        // 对 CSS/JS/SVG 与 JSON API 做 gzip/br 压缩协商；未发 Accept-Encoding 的客户端不受影响。
        .layer(CompressionLayer::new())
        .with_state(state);

    info!("完成本地 Web UI 路由组装");

    /*
     * ========================================================================
     * 步骤2：绑定本地监听端口
     * ========================================================================
     * 目标：
     * 1) 监听调用方指定的 IPv4 地址
     * 2) 复用既有端口探测顺序
     * 3) 命中端口后立即后台启动 axum 服务
     */
    info!("开始绑定本地 Web UI 监听端口");

    // 2.1 根据优先端口或默认端口组探测指定的监听地址
    let ports = if let Some(port) = preferred_port {
        vec![port]
    } else {
        vec![37421, 37422, 37423, 0]
    };

    // 2.2 命中可用端口后启动服务并返回最终监听地址
    let mut bind_errors = Vec::new();
    for port in ports {
        let attempted_addr = SocketAddr::new(bind_ip, port);
        match TcpListener::bind(attempted_addr).await {
            Ok(listener) => {
                let addr = listener.local_addr()?;
                let shutdown = CancellationToken::new();
                let shutdown_signal = shutdown.clone();
                let task = tokio::spawn(async move {
                    axum::serve(listener, app)
                        .with_graceful_shutdown(shutdown_signal.cancelled_owned())
                        .await
                });
                info!(%addr, "完成本地 Web UI 监听端口绑定");
                return Ok(BoundWebServer {
                    addr,
                    shutdown,
                    task: Some(task),
                });
            }
            Err(err) => bind_errors.push(format!("{attempted_addr}: {err}")),
        }
    }

    bail!(
        "Unable to bind the Web dashboard listener; attempts: {}",
        bind_errors.join("; ")
    );
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

async fn index_live() -> Html<&'static str> {
    // 根页面只依赖编译期版本号与 registry，进程内内容固定，生成一次后复用。
    static INDEX_HTML: OnceLock<String> = OnceLock::new();
    Html(INDEX_HTML.get_or_init(live_index_html))
}

async fn asset_file(Path(path): Path<String>, headers: HeaderMap) -> Response {
    let normalized = path.trim_start_matches('/');
    match assets::find_asset(normalized) {
        Some(asset) => asset.as_response(&headers),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn api_dashboard(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = dashboard_filter_from_params(&params);
    let scope = dashboard_scope_from_params(&params);
    let window = dashboard_window_from_params(&params).to_string();
    api_json_async(
        "/api/dashboard",
        load_dashboard_snapshot_resilient(state, filter, scope, window),
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
        load_via_dashboard(state, "overview", move |d| d.overview(&filter)),
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
        load_via_dashboard(state, "trends", move |d| d.trends(&window, &filter)),
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
        load_via_dashboard(state, "trends-daily", move |d| d.trends_daily(&filter)),
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
        load_via_dashboard(state, "models", move |d| d.model_breakdown(&filter)),
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
        load_via_dashboard(state, "sources", move |d| d.source_breakdown(&filter)),
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
        load_via_dashboard(state, "projects", move |d| d.project_breakdown(&filter)),
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
        load_via_dashboard(state, "costs", move |d| d.cost_breakdown(&filter)),
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
            "activity",
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
        load_behavior_api(
            state,
            "tools",
            move |d| d.tool_breakdown(&filter),
            degraded_tools,
        ),
    )
    .await
}

async fn api_explorer(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let query = match explorer_query_from_params(&params) {
        Ok(query) => query,
        Err(detail) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "code": "invalid_query",
                        "message": "Explorer 查询参数无效",
                        "detail": detail,
                        "endpoint": "/api/explorer",
                    }
                })),
            )
                .into_response();
        }
    };
    api_json_async(
        "/api/explorer",
        load_via_dashboard(state, "explorer", move |dashboard| {
            dashboard.explorer(&query)
        }),
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
        load_behavior_api(
            state,
            "optimize",
            move |d| d.optimize(&filter),
            degraded_optimize,
        ),
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
        load_via_dashboard(state, "compare-models", move |d| d.compare_models(&filter)),
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
            "compare",
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
        load_via_dashboard(state, "home-overview", move |d| d.home_overview(&filter)),
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
        load_via_dashboard(state, "heatmap", move |d| d.heatmap(&filter, days)),
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
        load_via_dashboard(state, "logs", move |d| d.logs(&query)),
    )
    .await
}

async fn api_health(State(state): State<WebState>) -> Response {
    api_json_async(
        "/api/health",
        load_via_dashboard(state, "health", |d| d.health()),
    )
    .await
}

async fn api_diagnostics(State(state): State<WebState>) -> Response {
    api_json_async("/api/diagnostics", load_diagnostics_cached(&state)).await
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
    headers: HeaderMap,
    Json(payload): Json<ForgetRequest>,
) -> Response {
    if let Some(response) = reject_non_local_write(&headers) {
        return response;
    }
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
        Ok(()) => {
            state.diagnostics_cache.invalidate();
            Json(json!({
                "ok": true,
                "source": source.as_str(),
                "file_path": payload.file_path,
            }))
            .into_response()
        }
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
    headers: HeaderMap,
    Json(options): Json<SyncOptions>,
) -> Response {
    if let Some(response) = reject_non_local_write(&headers) {
        return response;
    }
    let (job_id, _rx) = match state.jobs.try_start(&state.store, options) {
        Ok(started) => started,
        Err(rejected) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": {
                        "code": "sync_job_active",
                        "message": "已有同步任务正在运行或取消中",
                        "active_job_id": rejected.active_job_id,
                    }
                })),
            )
                .into_response();
        }
    };
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

async fn api_jobs_cancel(
    State(state): State<WebState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = reject_non_local_write(&headers) {
        return response;
    }
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

fn reject_non_local_write(headers: &HeaderMap) -> Option<Response> {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(normalize_authority)
    else {
        return Some(write_guard_error(
            "invalid_host",
            "本地写入 API 需要有效 Host header",
            None,
        ));
    };
    if !is_loopback_authority(&host) {
        return Some(write_guard_error(
            "invalid_host",
            "本地写入 API 只接受 localhost/loopback Host",
            Some(host),
        ));
    }

    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        let Some(origin_authority) = origin_authority(origin) else {
            return Some(write_guard_error(
                "invalid_origin",
                "本地写入 API 不接受无法解析的 Origin",
                Some(origin.to_string()),
            ));
        };
        if origin_authority != host {
            return Some(write_guard_error(
                "origin_mismatch",
                "本地写入 API 只接受同源 Origin",
                Some(origin.to_string()),
            ));
        }
    }

    None
}

fn write_guard_error(code: &str, message: &str, detail: Option<String>) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": {
                "code": code,
                "message": message,
                "detail": detail,
            }
        })),
    )
        .into_response()
}

fn origin_authority(origin: &str) -> Option<String> {
    let origin = origin.trim();
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))?;
    let authority = rest
        .split('/')
        .next()
        .map(normalize_authority)
        .filter(|value| !value.is_empty())?;
    Some(authority)
}

fn normalize_authority(raw: &str) -> String {
    raw.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn is_loopback_authority(authority: &str) -> bool {
    let host = authority_host(authority);
    host == "localhost" || host.parse::<IpAddr>().is_ok_and(|addr| addr.is_loopback())
}

fn authority_host(authority: &str) -> &str {
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    authority.split(':').next().unwrap_or_default()
}

struct DashboardQueryGuard {
    cancelled: Arc<AtomicBool>,
    interrupt: Option<InterruptHandle>,
    armed: bool,
}

impl DashboardQueryGuard {
    fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancelled,
            interrupt: None,
            armed: true,
        }
    }

    fn attach(&mut self, interrupt: InterruptHandle) {
        self.interrupt = Some(interrupt);
    }

    fn interrupt(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(interrupt) = &self.interrupt {
            interrupt.interrupt();
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
        self.interrupt = None;
    }
}

impl Drop for DashboardQueryGuard {
    fn drop(&mut self) {
        if self.armed {
            self.interrupt();
        }
    }
}

async fn load_via_dashboard<T, F>(state: WebState, section: &'static str, f: F) -> LlmusageResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
{
    load_via_dashboard_with_timeout(state, section, WEB_API_TIMEOUT, f).await
}

/// Loads `Dashboard::diagnostics()` through the request-boundary cache.
///
/// TTL hits return a clone without touching the database; a miss/expiry runs
/// one cold `Dashboard::diagnostics()` via the normal permit/spawn_blocking
/// path while concurrent callers wait on the single-flight mutex and reuse
/// the result. Sync job terminal states invalidate the entry through the
/// `JobRegistry` hook registered in `WebState`.
async fn load_diagnostics_cached(state: &WebState) -> LlmusageResult<DiagnosticsPayload> {
    if let Some(payload) = state.diagnostics_cache.get_fresh() {
        return Ok(payload);
    }
    let _compute = state.diagnostics_cache.compute.lock().await;
    loop {
        if let Some(payload) = state.diagnostics_cache.get_fresh() {
            return Ok(payload);
        }
        let generation = state.diagnostics_cache.generation();
        let payload = load_via_dashboard(state.clone(), "diagnostics", |dashboard| {
            dashboard.diagnostics()
        })
        .await?;
        if state
            .diagnostics_cache
            .store_if_generation(payload.clone(), generation)
        {
            return Ok(payload);
        }
        // A sync job reached a terminal state while the cold read was in
        // flight. Recompute under the same single-flight lock instead of
        // publishing or returning the pre-sync payload.
    }
}

async fn load_via_dashboard_with_timeout<T, F>(
    state: WebState,
    section: &'static str,
    timeout: Duration,
    f: F,
) -> LlmusageResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
{
    let started = Instant::now();
    let permit = match tokio::time::timeout(
        timeout,
        state.dashboard_query_semaphore.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => {
            return Err(LlmusageError::ConfigInvalid {
                detail: "dashboard query semaphore is closed".to_string(),
            });
        }
        Err(_) => {
            debug!(
                section,
                semaphore_wait_ms = started.elapsed().as_millis(),
                query_ms = 0_u128,
                cancelled = true,
                "Dashboard query timed out waiting for a permit"
            );
            return Err(dashboard_timeout_error(timeout));
        }
    };
    let semaphore_wait = started.elapsed();
    let Some(remaining) = timeout.checked_sub(started.elapsed()) else {
        return Err(dashboard_timeout_error(timeout));
    };
    if remaining.is_zero() {
        return Err(dashboard_timeout_error(timeout));
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let blocking_cancelled = Arc::clone(&cancelled);
    let (interrupt_tx, interrupt_rx) = oneshot::channel();
    let query_started = Instant::now();
    let mut task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let dashboard = Dashboard::open_with_busy_timeout(&state.store, WEB_READ_BUSY_TIMEOUT)?;
        let interrupt = dashboard.interrupt_handle();
        if blocking_cancelled.load(Ordering::SeqCst) {
            interrupt.interrupt();
            return Err(dashboard_cancelled_error());
        }
        if interrupt_tx.send(interrupt).is_err() {
            return Err(dashboard_cancelled_error());
        }
        if blocking_cancelled.load(Ordering::SeqCst) {
            return Err(dashboard_cancelled_error());
        }
        f(&dashboard)
    });
    let mut guard = DashboardQueryGuard::new(cancelled);
    let interrupt = match tokio::time::timeout(remaining, interrupt_rx).await {
        Ok(Ok(interrupt)) => interrupt,
        Ok(Err(_)) => {
            guard.disarm();
            return dashboard_join_result(task.await);
        }
        Err(_) => {
            guard.interrupt();
            let _ = task.await;
            guard.disarm();
            debug!(
                section,
                semaphore_wait_ms = semaphore_wait.as_millis(),
                query_ms = query_started.elapsed().as_millis(),
                cancelled = true,
                "Dashboard query timed out before SQLite became interruptible"
            );
            return Err(dashboard_timeout_error(timeout));
        }
    };
    guard.attach(interrupt);

    let Some(query_remaining) = timeout.checked_sub(started.elapsed()) else {
        guard.interrupt();
        let _ = task.await;
        guard.disarm();
        return Err(dashboard_timeout_error(timeout));
    };
    let result = match tokio::time::timeout(query_remaining, &mut task).await {
        Ok(joined) => {
            guard.disarm();
            dashboard_join_result(joined)
        }
        Err(_) => {
            guard.interrupt();
            let _ = task.await;
            guard.disarm();
            Err(dashboard_timeout_error(timeout))
        }
    };
    debug!(
        section,
        semaphore_wait_ms = semaphore_wait.as_millis(),
        query_ms = query_started.elapsed().as_millis(),
        cancelled = result
            .as_ref()
            .is_err_and(|error| matches!(error, LlmusageError::Cancelled { .. })),
        "Dashboard query completed"
    );
    result
}

fn dashboard_join_result<T>(
    joined: std::result::Result<LlmusageResult<T>, tokio::task::JoinError>,
) -> LlmusageResult<T> {
    joined.map_err(|err| LlmusageError::ConfigInvalid {
        detail: format!("blocking dashboard task failed: {err}"),
    })?
}

async fn load_behavior_api<T, F, D>(
    state: WebState,
    section: &'static str,
    f: F,
    degraded: D,
) -> LlmusageResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Dashboard) -> LlmusageResult<T> + Send + 'static,
    D: FnOnce(String) -> T,
{
    match load_via_dashboard_with_timeout(state, section, WEB_BEHAVIOR_API_TIMEOUT, f).await {
        Ok(value) => Ok(value),
        Err(err) => Ok(degraded(err.to_string())),
    }
}

fn dashboard_timeout_error(duration: Duration) -> LlmusageError {
    LlmusageError::ConfigInvalid {
        detail: dashboard_timeout_message(duration),
    }
}

fn dashboard_cancelled_error() -> LlmusageError {
    LlmusageError::Cancelled {
        operation: "dashboard query",
    }
}

fn dashboard_timeout_message(duration: Duration) -> String {
    format!(
        "dashboard query exceeded {} ms timeout",
        duration.as_millis()
    )
}

async fn load_dashboard_snapshot_resilient(
    state: WebState,
    filter: QueryFilter,
    scope: DashboardScope,
    window: String,
) -> LlmusageResult<serde_json::Value> {
    if scope == DashboardScope::Interactive {
        let diagnostics = load_diagnostics_cached(&state).await?;
        let interactive = load_via_dashboard(state, "dashboard:interactive", move |dashboard| {
            dashboard.interactive_snapshot_with_diagnostics(&filter, &window, &diagnostics)
        })
        .await?;
        return Ok(json!(interactive));
    }

    let diagnostics = load_diagnostics_cached(&state).await?;
    let core = load_via_dashboard(state.clone(), "dashboard:core", {
        let filter = filter.clone();
        move |dashboard| dashboard.core_snapshot_with_diagnostics(&filter, &diagnostics)
    })
    .await?;

    if scope == DashboardScope::Core {
        return Ok(dashboard_core_json(core));
    }

    let activity_filter = filter.clone();
    let activity = load_behavior_api(
        state.clone(),
        "dashboard:activity",
        move |dashboard| dashboard.activity_breakdown(&activity_filter),
        degraded_activity,
    );
    let tools_filter = filter.clone();
    let tools = load_behavior_api(
        state.clone(),
        "dashboard:tools",
        move |dashboard| dashboard.tool_breakdown(&tools_filter),
        degraded_tools,
    );
    let optimize_filter = filter.clone();
    let optimize = load_behavior_api(
        state.clone(),
        "dashboard:optimize",
        move |dashboard| dashboard.optimize(&optimize_filter),
        degraded_optimize,
    );
    let explorer_filter = filter.clone();
    let explorer = load_via_dashboard(state.clone(), "dashboard:explorer", move |dashboard| {
        dashboard.explorer(&ExplorerQuery {
            filter: explorer_filter,
            ..Default::default()
        })
    });
    let compare = load_behavior_api(
        state,
        "dashboard:compare",
        move |dashboard| dashboard.model_compare(&filter, None, None),
        degraded_compare,
    );
    let (activity, tools, optimize, explorer, compare) =
        tokio::join!(activity, tools, optimize, explorer, compare);
    let activity = activity?;
    let tools = tools?;
    let optimize = optimize?;
    let explorer = explorer?;
    let compare = compare?;

    let mut payload = dashboard_core_json(core);
    if let Some(object) = payload.as_object_mut() {
        object.insert("activity".to_string(), json!(activity));
        object.insert("tools".to_string(), json!(tools));
        object.insert("optimize".to_string(), json!(optimize));
        object.insert("explorer".to_string(), json!(explorer));
        object.insert("compare".to_string(), json!(compare));
    }
    Ok(payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardScope {
    Full,
    Core,
    Interactive,
}

fn dashboard_scope_from_params(params: &HashMap<String, String>) -> DashboardScope {
    match params.get("scope").map(String::as_str) {
        Some("core") => DashboardScope::Core,
        Some("interactive") => DashboardScope::Interactive,
        _ => DashboardScope::Full,
    }
}

fn dashboard_window_from_params(params: &HashMap<String, String>) -> &'static str {
    match params
        .get("window")
        .or_else(|| params.get("range"))
        .map(String::as_str)
    {
        Some("week" | "7d") => "week",
        Some("month" | "30d") => "month",
        Some("all") => "all",
        _ => "day",
    }
}

fn dashboard_core_json(core: crate::query::DashboardCoreSnapshot) -> serde_json::Value {
    json!({
        "overview": core.overview,
        "sync_command_center": core.sync_command_center,
        "day_trends": core.day_trends,
        "week_trends": core.week_trends,
        "month_trends": core.month_trends,
        "all_trends": core.all_trends,
        "models": core.models,
        "sources": core.sources,
        "projects": core.projects,
        "costs": core.costs,
        "health": core.health,
        "diagnostics": core.diagnostics,
    })
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

fn explorer_query_from_params(
    params: &HashMap<String, String>,
) -> std::result::Result<ExplorerQuery, String> {
    let filter = dashboard_filter_from_params(params);
    let granularity = match params.get("granularity").map(String::as_str) {
        Some(raw) => ExplorerGranularity::parse(raw)
            .ok_or_else(|| format!("unsupported granularity: {raw}"))?,
        None => ExplorerGranularity::Day,
    };
    let metric = match params.get("metric").map(String::as_str) {
        Some(raw) => {
            ExplorerMetric::parse(raw).ok_or_else(|| format!("unsupported metric: {raw}"))?
        }
        None => ExplorerMetric::AttributedCostUsd,
    };
    let group_by = match params.get("group_by").map(String::as_str) {
        Some(raw) => {
            ExplorerDimension::parse(raw).ok_or_else(|| format!("unsupported group_by: {raw}"))?
        }
        None => ExplorerDimension::Source,
    };
    let token_type = match params.get("token_type").map(String::as_str) {
        Some(raw) => Some(
            ExplorerTokenType::parse(raw)
                .ok_or_else(|| format!("unsupported token_type: {raw}"))?,
        ),
        None => None,
    };
    let limit = params
        .get("limit")
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(8);

    Ok(ExplorerQuery {
        filter,
        granularity,
        metric,
        group_by,
        filters: ExplorerFilters {
            session_id: query_string(params, "session_id")
                .or_else(|| query_string(params, "session")),
            tool_name: query_string(params, "tool_name").or_else(|| query_string(params, "tool")),
            tool_kind: query_string(params, "tool_kind"),
            is_tool: params.get("is_tool").map(|raw| parse_bool_query(Some(raw))),
            token_type,
        },
        limit,
        include_other: !params.contains_key("include_other")
            || parse_bool_query(params.get("include_other")),
    })
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
        Ok(value) => {
            let started = Instant::now();
            match serde_json::to_vec(&value) {
                Ok(body) => {
                    debug!(
                        endpoint,
                        serialization_ms = started.elapsed().as_millis(),
                        payload_bytes = body.len(),
                        "Web API response serialized"
                    );
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        body,
                    )
                        .into_response()
                }
                Err(err) => {
                    error!(endpoint, error = %err, "Web API response serialization failed");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
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
        net::{IpAddr, Ipv4Addr, SocketAddr},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::{Duration, Instant},
    };

    use axum::{
        body::to_bytes,
        http::{HeaderMap, HeaderValue, StatusCode, header},
    };
    use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
    use tempfile::TempDir;

    use crate::{
        AppPaths, LlmusageError,
        query::{diagnostics_stat_calls, reset_diagnostics_stat_counter},
        store::Store,
        sync::{JobRegistry, JobStatus, SyncOptions},
        testing::Fixture,
    };

    use super::{
        DiagnosticsCache, WEB_READ_BUSY_TIMEOUT, WebState, api_json, asset_manifest, bind_server,
        live_index_html, load_diagnostics_cached, load_via_dashboard,
        load_via_dashboard_with_timeout, serve, serve_on, server_task_result, snapshot_index_html,
    };

    fn make_store() -> anyhow::Result<(TempDir, Store)> {
        let temp = TempDir::new()?;
        let paths = AppPaths::with_root(temp.path().join(".llmusage"))?;
        let store = Store::new(&paths)?;
        store.bootstrap()?;
        Ok((temp, store))
    }

    #[tokio::test]
    async fn public_listener_binds_all_ipv4_interfaces_and_serves_root() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve_on(store, Some(0), IpAddr::V4(Ipv4Addr::UNSPECIFIED)).await?;
        assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::UNSPECIFIED));

        let loopback = SocketAddr::from(([127, 0, 0, 1], addr.port()));
        let (status, _body) = route_text(loopback, "GET", "/").await?;
        assert_eq!(status, StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn explicit_occupied_port_returns_structured_bind_error() -> anyhow::Result<()> {
        let occupied = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = occupied.local_addr()?;
        let (_temp, store) = make_store()?;

        let err = bind_server(store, Some(addr.port()), IpAddr::V4(Ipv4Addr::LOCALHOST))
            .await
            .err()
            .expect("occupied port must fail");
        let message = format!("{err:#}");
        assert!(message.contains("Unable to bind"));
        assert!(message.contains(&addr.to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn owned_server_remains_available_until_bounded_shutdown() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let server = bind_server(store, Some(0), IpAddr::V4(Ipv4Addr::LOCALHOST)).await?;
        let addr = server.addr();
        let (status, _body) = route_text(addr, "GET", "/").await?;
        assert_eq!(status, StatusCode::OK);
        server.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn server_task_errors_and_panics_are_observable() {
        let failed = tokio::spawn(async { Err(std::io::Error::other("listener failed")) });
        let err = server_task_result(failed.await).expect_err("task error must propagate");
        assert!(format!("{err:#}").contains("listener failed"));

        let panicked: tokio::task::JoinHandle<std::io::Result<()>> =
            tokio::spawn(async { panic!("server panic") });
        let err = server_task_result(panicked.await).expect_err("panic must propagate");
        assert!(format!("{err:#}").contains("panicked"));
    }

    #[tokio::test]
    async fn shutdown_timeout_aborts_non_cooperative_server_task() {
        let task = tokio::spawn(std::future::pending::<std::io::Result<()>>());
        let server = super::BoundWebServer::from_test_task(task);
        let err = server
            .shutdown_with_timeout(Duration::from_millis(10))
            .await
            .expect_err("non-cooperative task must hit the bounded deadline");
        assert!(format!("{err:#}").contains("did not stop within"));
    }

    async fn route_json(
        addr: SocketAddr,
        method: &str,
        path: &str,
        body: Option<String>,
    ) -> anyhow::Result<(StatusCode, serde_json::Value)> {
        route_json_with_headers(addr, method, path, body, &[]).await
    }

    async fn route_json_with_headers(
        addr: SocketAddr,
        method: &str,
        path: &str,
        body: Option<String>,
        headers: &[(&str, &str)],
    ) -> anyhow::Result<(StatusCode, serde_json::Value)> {
        let method = method.to_string();
        let path = path.to_string();
        let headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let raw = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr)?;
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            let body = body.unwrap_or_default();
            let request = format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 {headers}\
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
        let payload = serde_json::from_str(&body)
            .map_err(|err| anyhow::anyhow!("invalid JSON response body {body:?}: {err}"))?;
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

    async fn route_bytes(
        addr: SocketAddr,
        path: &str,
        headers: &[(&str, &str)],
    ) -> anyhow::Result<(StatusCode, String, Vec<u8>)> {
        let path = path.to_string();
        let headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let raw = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr)?;
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            let request = format!(
                "GET {path} HTTP/1.1\r\n\
                 Host: {addr}\r\n\
                 {headers}\
                 Connection: close\r\n\r\n"
            );
            stream.write_all(request.as_bytes())?;
            let mut raw = Vec::new();
            stream.read_to_end(&mut raw)?;
            Ok::<_, anyhow::Error>(raw)
        })
        .await??;

        let split = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| anyhow::anyhow!("invalid HTTP response"))?;
        let head = String::from_utf8(raw[..split].to_vec())?;
        let status_code = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|raw| raw.parse::<u16>().ok())
            .ok_or_else(|| anyhow::anyhow!("invalid status line: {head:?}"))?;
        let body = decode_http_body_bytes(&head, &raw[split + 4..])?;
        Ok((StatusCode::from_u16(status_code)?, head, body))
    }

    fn decode_http_body_bytes(head: &str, body: &[u8]) -> anyhow::Result<Vec<u8>> {
        if !head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            return Ok(body.to_vec());
        }
        let mut rest = body;
        let mut decoded = Vec::new();
        while let Some(line_end) = rest.windows(2).position(|window| window == b"\r\n") {
            let size = usize::from_str_radix(std::str::from_utf8(&rest[..line_end])?.trim(), 16)?;
            rest = &rest[line_end + 2..];
            if size == 0 {
                break;
            }
            if rest.len() < size + 2 {
                return Err(anyhow::anyhow!("truncated chunked response"));
            }
            decoded.extend_from_slice(&rest[..size]);
            rest = &rest[size + 2..];
        }
        Ok(decoded)
    }

    fn response_header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
        head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }

    #[test]
    fn live_shell_uses_module_entry() {
        let html = live_index_html();
        assert!(html.contains("data-mode=\"live\""));
        assert!(html.contains("data-app-version=\""));
        assert!(html.contains(
            "data-supported-sources=\"codex, claude, opencode, antigravity, kimi_code, pi\""
        ));
        assert!(html.contains("type=\"module\""));
        assert!(html.contains("assets/app.js"));
        assert!(html.contains("window.__LLMUSAGE_BOOTSTRAP__"));
        assert!(html.contains("CLAIM_TIMEOUT_MS = 3000"));
        assert!(html.contains("READY_TIMEOUT_MS = 1000"));
        assert!(html.contains("PROBE_TIMEOUT_MS = 1500"));
        assert!(html.contains("window.fetch('/', { cache: 'no-store'"));
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
        assert!(html.contains("mode === 'snapshot'"));
        assert!(html.contains("renderFailure('snapshot'"));
    }

    #[test]
    fn dashboard_shell_and_assets_wire_sync_command_center() {
        let html = live_index_html();
        assert!(html.contains("id=\"sync-command-center\""));
        assert!(html.contains("data-i18n=\"shell.syncCenter.eyebrow\""));

        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("renderDashboardLoadInstrument, renderSyncCommandCenter"));
        assert!(app_js.contains("from './render/sync-command-center.js';"));
        assert!(app_js.contains("renderSyncCommandCenter(context, dashboardState)"));
        assert!(app_js.contains("refreshSyncCommandCenter(state)"));

        let renderer = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/sync-command-center.js")
            .expect("sync command center renderer asset")
            .body;
        assert!(renderer.contains("export function renderSyncCommandCenter"));
        assert!(renderer.contains("activeJobSnapshot?.last_event"));
        assert!(renderer.contains("document.getElementById('btn-sync')?.click()"));
        assert!(renderer.contains("sync-command-center-segmented-bar"));
        assert!(renderer.contains("sync-command-center-details"));
        assert!(renderer.contains("sync-command-center-detail-grid"));
        assert!(renderer.contains("copy.detailsHint"));
        assert!(renderer.contains("sourceSegmentedBar(center)"));
        assert!(renderer.contains("sync-command-center-secondary"));
        assert!(renderer.contains("sync-command-center-status"));
        assert!(renderer.contains("last_run"));
        assert!(renderer.contains("current_job"));
        assert!(renderer.contains("activeJobSnapshot?.status !== 'running'"));
        assert!(renderer.contains("return { kind, source, summary, stats }"));
        assert!(renderer.contains("copy.sourceShareAria"));
        assert!(renderer.contains("statusLabels"));
        for forbidden in [
            "snapshot.error",
            "current.error",
            "last_run.error",
            "source.last_error",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "sync command center renderer must not expose raw error marker: {forbidden}"
            );
        }
        assert!(renderer.contains("error_key"));

        let copy_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "copy.js")
            .expect("copy.js asset")
            .body;
        assert!(copy_js.contains("syncCenter.headline.ready"));
        assert!(copy_js.contains("insertedDelta"));
        assert!(copy_js.contains("rebuild_risk"));
        assert!(copy_js.contains("available"));
        assert!(copy_js.contains("sourceShareAria"));
        assert!(copy_js.contains("detailsHint"));
        assert!(copy_js.contains("statusLabels"));
        assert!(copy_js.contains("syncCenter.reason.sourceError"));
    }

    #[test]
    fn sync_command_center_does_not_parse_human_summary_strings() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        let renderer = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/sync-command-center.js")
            .expect("sync command center renderer asset")
            .body;
        let combined = format!("{app_js}\n{renderer}");

        for forbidden in [
            "summary.match",
            "summary.split",
            "split('inserted_delta')",
            "split(\"inserted_delta\")",
            "inserted_delta=",
            "stored_events=",
        ] {
            assert!(
                !combined.contains(forbidden),
                "forbidden summary parsing marker: {forbidden}"
            );
        }
        assert!(renderer.contains("total_inserted"));
        assert!(renderer.contains("stored_events"));
        assert!(!renderer.contains("snapshot.summary"));
        assert!(!renderer.contains("last_run.summary"));
        for forbidden in [
            "snapshot.error",
            "current.error",
            "last_run.error",
            "source.last_error",
        ] {
            assert!(
                !renderer.contains(forbidden),
                "forbidden raw error marker: {forbidden}"
            );
        }
    }

    #[test]
    fn dashboard_assets_style_sync_command_center_responsively() {
        let components_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "components.css")
            .expect("components.css asset")
            .body;
        let layout_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "layout.css")
            .expect("layout.css asset")
            .body;
        let components_css_lf = components_css.replace("\r\n", "\n");

        assert!(components_css.contains(".sync-command-center"));
        assert!(components_css.contains(".sync-command-center-details summary"));
        assert!(components_css.contains(".sync-command-center-detail-grid"));
        assert!(components_css.contains(".sync-command-center-segments"));
        assert!(components_css.contains(".sync-command-center-source[data-tone='warn']"));
        assert!(components_css.contains(".sync-command-center-action .btn"));
        assert!(components_css.contains(".sync-command-center-metric .mini-label"));
        assert!(
            components_css_lf
                .contains(".explorer-controls,\n  .explorer-summary,\n  .explorer-results-grid")
        );
        assert!(components_css.contains("--sync-tone: var(--danger);"));
        assert!(components_css.contains("color-mix(in oklab, var(--danger) 20%, transparent)"));
        assert!(layout_css.contains(".sync-command-center-detail-grid"));
        assert!(layout_css.contains("@media (max-width: 720px)"));
        assert!(!components_css.contains("border-left-color: var(--sync-tone"));
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
                "load-state.js",
                "copy.js",
                "i18n.js",
                "theme.js",
                "runtime.js",
                "data.js",
                "data/fetch.js",
                "data/format.js",
                "data/derive.js",
                "data/render-key.js",
                "render/hero.js",
                "render/sync-command-center.js",
                "render/trends.js",
                "render/models.js",
                "render/sources.js",
                "render/projects.js",
                "render/behavior.js",
                "render/explorer.js",
                "render/costs.js",
                "render/insights.js",
                "favicon.svg",
            ]
        );
        assert!(paths.iter().all(|path| !path.contains("fingerprint")));
    }

    #[test]
    fn dashboard_assets_use_compact_token_formatter() {
        let asset = |path: &str| {
            asset_manifest()
                .iter()
                .find(|asset| asset.path == path)
                .unwrap_or_else(|| panic!("{path} asset"))
                .body
        };

        let format_js = asset("data/format.js");
        assert!(format_js.contains("const COMPACT_UNITS"));
        assert!(format_js.contains("suffix: 'K'"));
        assert!(format_js.contains("suffix: 'M'"));
        assert!(format_js.contains("suffix: 'B'"));
        assert!(format_js.contains("export function formatTokenAmount(value)"));
        assert!(format_js.contains("Number(scaled.toFixed(maximumFractionDigits)) >= 1000"));

        let data_js = asset("data.js");
        assert!(data_js.contains("formatTokenAmount,"));

        let models_js = asset("render/models.js");
        assert!(models_js.contains("formatTokenAmount(total_tokens)"));
        assert!(
            models_js.contains(r#"title="${escapeHtml(`${formatNumber(total_tokens)} Token`)}""#)
        );

        let sources_js = asset("render/sources.js");
        assert!(sources_js.contains("const compactTokens = formatTokenAmount(total_tokens);"));
        assert!(sources_js.contains("const exactTokens = `${formatNumber(total_tokens)} Token`;"));

        let trends_js = asset("render/trends.js");
        assert!(trends_js.contains("const valueLabel = formatTokenAmount(value);"));
        assert!(trends_js.contains("formatTokenAmount(row.total_tokens || 0)"));
    }

    #[test]
    fn render_assets_use_updated_terms() {
        let selected_bodies = asset_manifest()
            .iter()
            .filter(|asset| matches!(asset.path, "copy.js" | "render/hero.js"))
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
    fn hero_health_disclosure_tracks_the_mobile_breakpoint() {
        let hero_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/hero.js")
            .expect("hero.js asset")
            .body;
        assert!(hero_js.contains("STATUS_PANEL_MOBILE_QUERY = '(max-width: 720px)'"));
        assert!(hero_js.contains("details.open = !statusPanelMediaQuery.matches"));
        assert!(hero_js.contains("addEventListener('change', syncStatusPanelDisclosure)"));
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
    fn dashboard_data_layers_pass_through_sync_command_center() {
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

        assert!(fetch_js.contains("sync_command_center: snapshot?.sync_command_center"));
        assert!(fetch_js.contains("sync_command_center: null"));
        assert!(derive_js.contains("sync_command_center"));
        assert!(derive_js.contains("function normalizeSyncCommandCenter"));
        assert!(
            derive_js
                .contains("syncCommandCenter: normalizeSyncCommandCenter(sync_command_center)")
        );
        assert!(!derive_js.contains("last_error"));
    }

    #[test]
    fn app_entry_loads_dashboard_sections_instead_of_missing_window_global() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert!(app_js.contains("loadDashboardSnapshot(state)"));
        assert!(app_js.contains("loadDashboardProgressive(state)"));
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
        assert!(
            fetch_js
                .contains("loadLiveJson(`/api/dashboard${buildDashboardQuery(state, options)}`")
        );
        assert!(fetch_js.contains("export async function loadSection"));
        assert!(fetch_js.contains("state?.rangePreset"));
        assert!(fetch_js.contains("params.set('range', state.rangePreset)"));
        assert!(fetch_js.contains("回退到旧分段 API"));
        assert!(fetch_js.contains("options.legacyFallback === false"));
        assert!(fetch_js.contains("snapshot.json"));
    }

    #[test]
    fn dashboard_assets_coalesce_cache_and_fast_refresh_ranges() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
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
        let behavior_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/behavior.js")
            .expect("render/behavior.js asset")
            .body;
        let explorer_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/explorer.js")
            .expect("render/explorer.js asset")
            .body;

        assert!(fetch_js.contains("const LIVE_CACHE_TTL_MS = 10000"));
        assert!(fetch_js.contains("const LIVE_CACHE_MAX_ENTRIES = 32"));
        assert!(fetch_js.contains("const liveInflight = new Map()"));
        assert!(fetch_js.contains("function normalizedRequestKey"));
        assert!(fetch_js.contains("export function clearLiveRequestCache"));
        assert!(fetch_js.contains("liveCacheEpoch += 1"));
        assert!(fetch_js.contains("export async function loadDashboardInteractiveSnapshot"));
        assert!(fetch_js.contains("scope: 'interactive'"));
        assert!(fetch_js.contains("entry.controller.abort()"));
        let data_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data.js")
            .expect("data.js asset")
            .body;
        assert!(data_js.contains("loadDashboardInteractiveSnapshot"));
        assert!(data_js.contains("loadDashboardSecondarySections"));
        assert!(app_js.contains("async function reloadDashboardFastRange"));
        assert!(app_js.contains("loadDashboardInteractiveSnapshot(state, {"));
        assert!(app_js.contains("legacyFallback: false"));
        assert!(app_js.contains("const SECONDARY_LOAD_CONCURRENCY = 2"));
        assert!(app_js.contains("runSecondaryLoaders(loaders"));
        assert!(app_js.contains("renderDashboard(state.rawData)"));
        assert!(app_js.contains("sameStableFilters(previousFilters"));
        assert!(app_js.contains("clearLiveRequestCache()"));
        assert!(app_js.contains("!hasUsableExplorer(snapshot) || !isDefaultExplorerState(state)"));
        assert!(derive_js.contains("secondary_refreshing: Boolean(_meta?.secondary_refreshing)"));
        assert!(behavior_js.contains("shell.refresh.secondaryStale"));
        assert!(explorer_js.contains("shell.refresh.secondaryStale"));
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
            "data-i18n=\"shell.nav.item.explorer\"",
            "data-i18n=\"shell.nav.item.cost\"",
            "data-i18n=\"shell.behavior.optimize.title\"",
            "data-i18n=\"shell.behavior.compare.title\"",
            "data-i18n=\"shell.explorer.title\"",
            "data-i18n=\"shell.explorer.metric\"",
            "data-i18n=\"shell.explorer.groupBy\"",
            "data-i18n=\"shell.explorer.apply\"",
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
        assert!(app_js.contains("setupExplorerControls(state)"));
        assert!(app_js.contains("renderExplorer(context, dashboardState)"));
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

        let data_change_reload = app_js
            .split("async function reloadDashboardAfterDataChange")
            .nth(1)
            .and_then(|tail| tail.split("async function refreshDashboardInPlace").next())
            .expect("reloadDashboardAfterDataChange function body");
        assert!(data_change_reload.contains("return reloadDashboardFastRange(state, options)"));
        assert!(
            !data_change_reload.contains("return reloadDashboard(state)"),
            "live auto-refresh and sync completion must not fall back to full scope"
        );
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
        assert!(app_js.contains("clearLiveRequestCache()"));
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
            .split("  try {\n    // 1.2 live")
            .next()
            .expect("main load marker");
        for marker in [
            "setupNavigation()",
            "setupFilterControls(state)",
            "setupExplorerControls(state)",
            "setupPanelToggles(state)",
            "setupSyncJob(state)",
            "setupThemeToggle()",
            "setupLocaleToggle(state)",
            "setupDashboardRetry(state)",
            "window.__LLMUSAGE_BOOTSTRAP__?.ready?.()",
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
        assert!(fetch_js.contains("loadOptionalExplorer(state)"));
        assert!(fetch_js.contains("loadOptionalSection(state, 'compare'"));
        assert!(fetch_js.contains("level: 'degraded'"));
    }

    #[test]
    fn dashboard_assets_wire_explorer_workbench_without_frontend_pivoting() {
        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
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
        let explorer_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/explorer.js")
            .expect("render/explorer.js asset")
            .body;
        let copy_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "copy.js")
            .expect("copy.js asset")
            .body;
        let components_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "components.css")
            .expect("components.css asset")
            .body;

        assert!(app_js.contains("DEFAULT_EXPLORER"));
        assert!(app_js.contains("currentExplorerInputs()"));
        assert!(app_js.contains("await loadExplorer(state)"));
        assert!(app_js.contains("shell.explorer.snapshotDisabled"));
        assert!(fetch_js.contains("export function buildExplorerQuery"));
        assert!(fetch_js.contains("export async function loadExplorer"));
        assert!(fetch_js.contains("snapshot?.explorer"));
        assert!(
            fetch_js.contains("loadLiveJson(`/api/explorer${buildExplorerQuery(state)}`, options)")
        );
        assert!(fetch_js.contains("return await loadExplorer(state, options);"));
        assert!(!fetch_js.contains("loadOptionalSection(state, 'explorer'"));
        assert!(derive_js.contains("const explorerPayload = normalizeExplorer(explorer);"));
        assert!(derive_js.contains("explorer: explorerPayload"));
        assert!(explorer_js.contains("renderExplorer"));
        assert!(explorer_js.contains("context?.panels?.explorer"));
        assert!(
            !explorer_js.contains("fetch("),
            "Explorer renderer must render backend payloads, not fetch or pivot raw data"
        );
        assert!(copy_js.contains("shell.explorer.includeNonTool"));
        assert!(components_css.contains(".explorer-controls"));
        assert!(components_css.contains(".explorer-results-grid"));
    }

    #[test]
    fn dashboard_assets_bound_explorer_time_series_height() {
        let html = live_index_html();
        let explorer_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "render/explorer.js")
            .expect("render/explorer.js asset")
            .body;
        let copy_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "copy.js")
            .expect("copy.js asset")
            .body;
        let components_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "components.css")
            .expect("components.css asset")
            .body;
        let layout_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "layout.css")
            .expect("layout.css asset")
            .body;
        let charts_css = asset_manifest()
            .iter()
            .find(|asset| asset.path == "charts.css")
            .expect("charts.css asset")
            .body;

        assert!(html.contains("id=\"explorer-series-chart\""));
        assert!(html.contains("id=\"explorer-series-details\""));
        assert!(!html.contains("id=\"explorer-series\""));
        assert!(explorer_js.contains("const MAX_CHART_SERIES = 5"));
        assert!(explorer_js.contains("const SERIES_TABLE_LIMIT = 80"));
        assert!(explorer_js.contains(".slice(0, MAX_CHART_SERIES)"));
        assert!(explorer_js.contains("series.slice(-SERIES_TABLE_LIMIT)"));
        assert!(explorer_js.contains("function miniChartGeometry(values)"));
        assert!(explorer_js.contains("function renderSeriesDetails(series, explorer, open)"));
        assert!(explorer_js.contains("detailsHost.querySelector('details')?.open"));
        assert_eq!(
            copy_js
                .matches("'shell.explorer.seriesIndependentScale'")
                .count(),
            2
        );
        assert!(copy_js.contains("'shell.explorer.seriesTruncated'"));
        assert_eq!(
            copy_js.matches("'shell.explorer.seriesScopeAll'").count(),
            2
        );
        assert!(components_css.contains(".explorer-series-chart-card"));
        assert!(components_css.contains("max-height: min(420px, 50vh)"));
        assert!(components_css.contains("position: sticky"));
        assert!(layout_css.contains("scroll-margin-top: 80px"));
        assert!(charts_css.contains(".explorer-series-line"));
        assert!(charts_css.contains(".explorer-series-peak-dot"));
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
        let derive_js = derive_js.replace("\r\n", "\n");
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
        assert!(charts_css.contains("min-width: 0"));
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
    async fn dashboard_queries_hold_semaphore_around_blocking_work() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let state = WebState::with_jobs_and_query_limit(store, Default::default(), 1);
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));

        let query = |state: WebState| {
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            load_via_dashboard_with_timeout(
                state,
                "test-semaphore",
                Duration::from_secs(2),
                move |_dashboard| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    let mut observed = max_active.load(Ordering::SeqCst);
                    while current > observed {
                        match max_active.compare_exchange(
                            observed,
                            current,
                            Ordering::SeqCst,
                            Ordering::SeqCst,
                        ) {
                            Ok(_) => break,
                            Err(actual) => observed = actual,
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok::<(), LlmusageError>(())
                },
            )
        };

        let (first, second) = tokio::join!(query(state.clone()), query(state));
        first?;
        second?;
        assert_eq!(
            max_active.load(Ordering::SeqCst),
            1,
            "dashboard blocking queries should be serialized by the test semaphore"
        );
        Ok(())
    }

    #[tokio::test]
    async fn dashboard_timeout_interrupts_sqlite_and_releases_permit() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let state = WebState::with_jobs_and_query_limit(store, Default::default(), 1);
        let started = Instant::now();
        let error = load_via_dashboard_with_timeout(
            state.clone(),
            "test-cancel",
            Duration::from_millis(20),
            |dashboard| dashboard.test_slow_query(),
        )
        .await
        .expect_err("the recursive SQLite query should exceed the test timeout");
        assert!(error.to_string().contains("dashboard query exceeded"));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "SQLite interruption should stop the blocking query promptly"
        );

        load_via_dashboard_with_timeout(
            state,
            "test-after-cancel",
            Duration::from_secs(1),
            |dashboard| dashboard.overview(&Default::default()).map(|_| ()),
        )
        .await?;
        Ok(())
    }

    #[test]
    fn web_read_connections_use_short_busy_timeout_while_store_default_stays_30s()
    -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let default_conn = store.open_connection()?;
        let default_ms: i64 =
            default_conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        assert_eq!(
            default_ms, 30_000,
            "sync writer / default connections keep the 30s busy timeout"
        );
        let web_conn = store.open_connection_with_busy_timeout(WEB_READ_BUSY_TIMEOUT)?;
        let web_ms: i64 = web_conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
        assert_eq!(web_ms, 1_500, "web read connections fail lock waits fast");
        Ok(())
    }

    #[tokio::test]
    async fn locked_database_surfaces_busy_error_within_request_budget() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        // Hold an exclusive lock so every other connection hits SQLITE_BUSY.
        let blocker = store.open_connection()?;
        blocker.execute_batch("PRAGMA locking_mode = EXCLUSIVE")?;
        blocker.execute_batch("BEGIN EXCLUSIVE")?;
        blocker.execute(
            "INSERT INTO meta(key, value) VALUES ('lock-test', '1') ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )?;

        let state = WebState::with_jobs_and_query_limit(store.clone(), Default::default(), 4);
        let started = Instant::now();
        let result = load_via_dashboard(state, "overview", |dashboard| {
            dashboard.overview(&Default::default()).map(|_| ())
        })
        .await;
        let elapsed = started.elapsed();
        blocker.execute_batch("ROLLBACK; PRAGMA locking_mode = NORMAL")?;

        let error = result.expect_err("a fully locked database must fail the web read");
        assert!(
            error.to_string().contains("locked"),
            "expected a lock-related error, got: {error}"
        );
        assert!(
            elapsed >= Duration::from_millis(1_200) && elapsed < Duration::from_secs(4),
            "web read should fail near the 1.5s busy timeout instead of the 5s request timeout, got {elapsed:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn diagnostics_cache_hits_within_ttl_and_recomputes_after_expiry() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(200, 1, 2)?;
        let state = WebState::with_diagnostics_cache_ttl(
            fixture.store().clone(),
            JobRegistry::default(),
            4,
            Duration::from_millis(60),
        );

        reset_diagnostics_stat_counter();
        let first = load_diagnostics_cached(&state).await?;
        assert_eq!(first.by_source.len(), 3);
        assert_eq!(
            diagnostics_stat_calls(),
            201,
            "cold load stats every source_file row once"
        );

        let second = load_diagnostics_cached(&state).await?;
        assert_eq!(
            diagnostics_stat_calls(),
            201,
            "TTL hit must serve a clone without any stat call"
        );
        assert_eq!(
            serde_json::to_value(&first)?,
            serde_json::to_value(&second)?
        );

        tokio::time::sleep(Duration::from_millis(90)).await;
        load_diagnostics_cached(&state).await?;
        assert_eq!(
            diagnostics_stat_calls(),
            402,
            "expired entry must recompute exactly one cold pass"
        );
        Ok(())
    }

    #[test]
    fn diagnostics_cache_invalidation_fences_in_flight_store() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(2, 0, 0)?;
        let payload = crate::query::Dashboard::open(fixture.store())?.diagnostics()?;
        let cache = DiagnosticsCache::new(Duration::from_secs(60));

        let stale_generation = cache.generation();
        cache.invalidate();
        assert!(
            !cache.store_if_generation(payload.clone(), stale_generation),
            "a cold read started before invalidation must not repopulate the cache"
        );
        assert!(cache.get_fresh().is_none());

        let current_generation = cache.generation();
        assert!(cache.store_if_generation(payload, current_generation));
        assert!(cache.get_fresh().is_some());
        Ok(())
    }

    #[tokio::test]
    async fn diagnostics_cache_detects_external_file_deletion_after_ttl() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(2, 0, 0)?;
        let state = WebState::with_diagnostics_cache_ttl(
            fixture.store().clone(),
            JobRegistry::default(),
            4,
            Duration::from_millis(60),
        );

        let first = load_diagnostics_cached(&state).await?;
        assert!(
            first
                .by_source
                .iter()
                .all(|source| source.missing_file_count == 0)
        );

        std::fs::remove_file(fixture.paths().root_dir.join("stress-files/file-0.jsonl"))?;

        let cached = load_diagnostics_cached(&state).await?;
        assert!(
            cached
                .by_source
                .iter()
                .all(|source| source.missing_file_count == 0),
            "inside the TTL window the cached payload is still served"
        );

        tokio::time::sleep(Duration::from_millis(90)).await;
        let recomputed = load_diagnostics_cached(&state).await?;
        let codex = recomputed
            .by_source
            .iter()
            .find(|source| source.source == "codex")
            .expect("codex diagnostics row");
        assert_eq!(
            codex.missing_file_count, 1,
            "after TTL expiry the externally deleted file is judged missing"
        );
        Ok(())
    }

    #[tokio::test]
    async fn diagnostics_cache_invalidates_when_sync_job_finishes() -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(50, 0, 1)?;
        let jobs = JobRegistry::default();
        let state = WebState::with_diagnostics_cache_ttl(
            fixture.store().clone(),
            jobs.clone(),
            4,
            // Long TTL: only the sync terminal hook may clear this entry.
            Duration::from_secs(3_600),
        );

        reset_diagnostics_stat_counter();
        load_diagnostics_cached(&state).await?;
        load_diagnostics_cached(&state).await?;
        assert_eq!(diagnostics_stat_calls(), 50, "second load is a TTL hit");

        let (job_id, _rx) = state.jobs.start(
            &state.store,
            SyncOptions {
                // Parser-less source: the job touches no real parser scan
                // roots and reaches a terminal state immediately.
                source: Some("antigravity".to_string()),
                ..Default::default()
            },
        );
        let wait_started = Instant::now();
        loop {
            let terminal = state
                .jobs
                .snapshot(&job_id)
                .map(|snapshot| {
                    matches!(
                        snapshot.status,
                        JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
                    )
                })
                .unwrap_or(false);
            if terminal {
                break;
            }
            anyhow::ensure!(
                wait_started.elapsed() < Duration::from_secs(30),
                "sync job did not reach a terminal state in time"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        // The hook fires synchronously right after the terminal state is set;
        // a short settle keeps this assertion race-free.
        tokio::time::sleep(Duration::from_millis(100)).await;

        load_diagnostics_cached(&state).await?;
        assert_eq!(
            diagnostics_stat_calls(),
            100,
            "sync job terminal state must invalidate the cached diagnostics"
        );
        Ok(())
    }

    #[tokio::test]
    async fn diagnostics_cache_single_flight_computes_once_under_concurrency() -> anyhow::Result<()>
    {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(2_000, 0, 1)?;
        let state = WebState::with_diagnostics_cache_ttl(
            fixture.store().clone(),
            JobRegistry::default(),
            4,
            Duration::from_secs(60),
        );

        reset_diagnostics_stat_counter();
        // `join!` polls every waiter to the single-flight mutex before the
        // first cold pass can finish, so all eight share one computation.
        let results = tokio::join!(
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
            load_diagnostics_cached(&state),
        );
        let payloads = [
            results.0?, results.1?, results.2?, results.3?, results.4?, results.5?, results.6?,
            results.7?,
        ];
        let first = serde_json::to_value(&payloads[0])?;
        for payload in &payloads {
            assert_eq!(&serde_json::to_value(payload)?, &first);
        }
        assert_eq!(
            diagnostics_stat_calls(),
            2_000,
            "8 concurrent cold loads must share exactly one stat pass"
        );
        Ok(())
    }

    #[tokio::test]
    async fn diagnostics_cache_is_shared_between_diagnostics_api_and_dashboard()
    -> anyhow::Result<()> {
        let fixture = Fixture::new()?;
        fixture.seed_stress_dashboard(500, 0, 2)?;
        let addr = serve(fixture.store().clone(), Some(0)).await?;

        reset_diagnostics_stat_counter();
        let (core_status, _core) =
            route_json(addr, "GET", "/api/dashboard?scope=core", None).await?;
        assert_eq!(core_status, StatusCode::OK);
        let stats_after_core = diagnostics_stat_calls();
        assert_eq!(stats_after_core, 500, "core dashboard warms the cache");

        let (status, payload) = route_json(addr, "GET", "/api/diagnostics", None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["by_source"].as_array().unwrap().len(), 3);
        assert_eq!(
            diagnostics_stat_calls(),
            stats_after_core,
            "/api/diagnostics must reuse the dashboard-warmed cache"
        );
        Ok(())
    }

    #[tokio::test]
    async fn write_apis_reject_cross_origin_posts() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;

        let (status, payload) = route_json_with_headers(
            addr,
            "POST",
            "/api/jobs",
            Some(serde_json::to_string(&SyncOptions::default())?),
            &[("Origin", "http://evil.example")],
        )
        .await?;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(payload["error"]["code"], "origin_mismatch");
        Ok(())
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
            VALUES ('codex:behavior:web:multi-tool', 'codex', 'gpt-5',
                    '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
                    100, 0, 0, 50, 0, 150,
                    'project-a', 'Project A', NULL, 'path-a',
                    'session-a', NULL, 'path-a', '2026-05-01T00:00:00Z',
                    1.0, 1.0, 'static', 'static-v1', NULL)
            "#,
            [],
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
            VALUES ('codex:behavior:web:non-tool', 'codex', 'gpt-5',
                    '2026-05-02T01:00:00Z', '2026-05-02T01:00:00Z',
                    80, 0, 0, 20, 0, 100,
                    'project-a', 'Project A', NULL, 'path-a',
                    'session-a', NULL, 'path-a', '2026-05-02T01:00:00Z',
                    0.25, 0.25, 'static', 'static-v1', NULL)
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
                    180, 0, 0, 70, 0, 250, 1.25, 1.25, 'static', 'static-v1',
                    2, '2026-05-01T00:00:00Z')
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
            ) VALUES ('turn:codex:behavior:web:multi-tool', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T00:00:00Z', 'coding',
                1, 0, 1, 1, 100, 0, 0, 50, 0, 150, '2026-05-01T00:00:00Z')
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
            ) VALUES ('turn:codex:behavior:web:non-tool', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T01:00:00Z', 'coding',
                0, 0, 0, 1, 80, 0, 0, 20, 0, 100, '2026-05-01T01:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES ('tool:codex:behavior:web:edit', 'turn:codex:behavior:web:multi-tool',
                'codex:behavior:web:multi-tool', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T00:00:00Z', 'Edit', 'edit',
                NULL, NULL, 'fp-edit', 'Edit src/web/mod.rs', '2026-05-01T00:00:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_tool_call(
                tool_call_key, turn_key, event_key, source, session_id,
                source_path_hash, project_hash, model, occurred_at, tool_name,
                tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
            ) VALUES ('tool:codex:behavior:web:read', 'turn:codex:behavior:web:multi-tool',
                'codex:behavior:web:multi-tool', 'codex', 'session-a', 'path-a',
                'project-a', 'gpt-5', '2026-05-01T00:00:00Z', 'Read', 'read',
                NULL, NULL, 'fp-read', 'Read src/web/mod.rs', '2026-05-01T00:00:00Z')
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
        let tool_rows = tools["breakdown"].as_array().expect("tool rows");
        assert_eq!(tool_rows.len(), 3);
        let total_cost: f64 = tool_rows
            .iter()
            .map(|row| row["estimated_cost_usd"].as_f64().unwrap_or_default())
            .sum();
        assert!((total_cost - 1.25).abs() < f64::EPSILON);
        let edit = tool_rows
            .iter()
            .find(|row| row["tool_name"] == "Edit")
            .expect("edit row");
        assert_eq!(edit["tool_kind"], "edit");
        assert_eq!(edit["call_share"], 0.5);
        assert_eq!(edit["estimated_cost_usd"], 0.5);
        let non_tool = tool_rows
            .iter()
            .find(|row| row["tool_name"] == "(non-tool)")
            .expect("non-tool row");
        assert_eq!(non_tool["tool_kind"], "(non-tool)");
        assert_eq!(non_tool["calls"], 0);
        assert_eq!(non_tool["turn_count"], 1);
        assert_eq!(non_tool["estimated_cost_usd"], 0.25);

        let (status, dashboard) =
            route_json(addr, "GET", &format!("/api/dashboard?{filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(dashboard["activity"]["breakdown"][0]["category"], "coding");
        assert_eq!(
            dashboard["activity"]["breakdown"][0]["estimated_cost_usd"],
            1.25
        );
        assert_eq!(dashboard["tools"]["breakdown"].as_array().unwrap().len(), 3);

        let day_two_filter = "source=codex&model=gpt-5&since=2026-05-02&until=2026-05-02";
        let (status, tools_day_two) =
            route_json(addr, "GET", &format!("/api/tools?{day_two_filter}"), None).await?;
        assert_eq!(status, StatusCode::OK);
        let day_two_rows = tools_day_two["breakdown"].as_array().expect("day two rows");
        assert_eq!(day_two_rows.len(), 1);
        assert_eq!(day_two_rows[0]["tool_name"], "(non-tool)");
        assert_eq!(day_two_rows[0]["estimated_cost_usd"], 0.25);
        Ok(())
    }

    #[tokio::test]
    async fn explorer_api_returns_grouped_rows_and_series() -> anyhow::Result<()> {
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
            VALUES ('codex:explorer:web:a', 'codex', 'gpt-5',
                    '2026-05-01T00:00:00Z', '2026-05-01T00:00:00Z',
                    120, 0, 0, 60, 0, 180,
                    'project-a', 'Project A', NULL, 'path-a',
                    'session-a', 'Session A', 'path-a', '2026-05-01T00:00:00Z',
                    1.0, 1.0, 'static', 'static-v1', NULL)
            "#,
            [],
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
            VALUES ('codex:explorer:web:b', 'codex', 'gpt-5',
                    '2026-05-02T00:00:00Z', '2026-05-02T00:00:00Z',
                    90, 0, 0, 30, 0, 120,
                    'project-a', 'Project A', NULL, 'path-b',
                    'session-b', 'Session B', 'path-b', '2026-05-02T00:00:00Z',
                    0.6, 0.6, 'static', 'static-v1', NULL)
            "#,
            [],
        )?;
        for (tool_call_key, event_key, session_id, occurred_at, tool_name, tool_kind) in [
            (
                "tool:explorer:web:read:a",
                "codex:explorer:web:a",
                "session-a",
                "2026-05-01T00:00:00Z",
                "Read",
                "read",
            ),
            (
                "tool:explorer:web:edit:a",
                "codex:explorer:web:a",
                "session-a",
                "2026-05-01T00:00:00Z",
                "Edit",
                "edit",
            ),
            (
                "tool:explorer:web:read:b",
                "codex:explorer:web:b",
                "session-b",
                "2026-05-02T00:00:00Z",
                "Read",
                "read",
            ),
        ] {
            conn.execute(
                r#"
                INSERT INTO usage_tool_call(
                    tool_call_key, turn_key, event_key, source, session_id,
                    source_path_hash, project_hash, model, occurred_at, tool_name,
                    tool_kind, mcp_server, mcp_tool, input_fingerprint, safe_preview, created_at
                ) VALUES (?1, ?2, ?3, 'codex', ?4, ?4, 'project-a', 'gpt-5',
                    ?5, ?6, ?7, NULL, NULL, ?1, ?6, ?5)
                "#,
                rusqlite::params![
                    tool_call_key,
                    format!("turn:{event_key}"),
                    event_key,
                    session_id,
                    occurred_at,
                    tool_name,
                    tool_kind
                ],
            )?;
        }
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(
            addr,
            "GET",
            "/api/explorer?source=codex&metric=attributed_cost_usd&group_by=session&granularity=day&tool_name=Read&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["support"]["level"], "normalized");
        assert_eq!(payload["metric"], "attributed_cost_usd");
        assert_eq!(payload["group_by"], "session");
        assert_eq!(payload["totals"]["value"], 1.1);
        let rows = payload["rows"].as_array().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["key"], "session-b");
        assert_eq!(rows[0]["value"], 0.6);
        assert_eq!(rows[1]["key"], "session-a");
        assert_eq!(rows[1]["value"], 0.5);
        assert_eq!(payload["series"].as_array().expect("series").len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn explorer_api_rejects_invalid_metric_values() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(addr, "GET", "/api/explorer?metric=bogus", None).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(payload["error"]["code"], "invalid_query");
        assert!(
            payload["error"]["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains("unsupported metric"))
        );
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
                    source, provider_label, model, hour_start, project_hash, project_label, project_ref,
                    input_tokens, cache_read_tokens, cache_creation_tokens,
                    output_tokens, reasoning_output_tokens, total_tokens,
                    cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                    event_count, updated_at
                )
                VALUES ('codex', '', ?1, '2026-05-01T00:00:00Z', 'project-a', 'Project A', NULL,
                        100, 10, 0, 50, 0, 160, 0.2, 0.2, 'static', 'static-v1',
                        1, '2026-05-01T00:00:00Z')
                ON CONFLICT(source, provider_label, model, hour_start, project_hash) DO UPDATE SET
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
    async fn api_dashboard_core_scope_omits_secondary_sections() -> anyhow::Result<()> {
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
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(
            addr,
            "GET",
            "/api/dashboard?scope=core&source=codex&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["overview"]["total"]["total_tokens"], 10);
        assert!(payload["models"].as_array().expect("models").len() == 1);
        assert!(payload["health"].is_object());
        assert!(payload["activity"].is_null());
        assert!(payload["tools"].is_null());
        assert!(payload["optimize"].is_null());
        assert!(payload["explorer"].is_null());
        assert!(payload["compare"].is_null());
        Ok(())
    }

    #[tokio::test]
    async fn api_dashboard_interactive_scope_is_lean_and_selected_window_only() -> anyhow::Result<()>
    {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start, project_hash, project_label, project_ref,
                input_tokens, cache_read_tokens, cache_creation_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                cost_with_cache_usd, cost_without_cache_usd, pricing_status, pricing_source,
                event_count, updated_at
            )
            VALUES ('codex', 'gpt-5', ?1, 'project-a', 'Project A', NULL,
                    10, 0, 0, 0, 0, 10, 0.1, 0.1, 'static', 'static-v1',
                    1, ?1)
            "#,
            [&now],
        )?;
        let mut insert_cursor = conn.prepare(
            "INSERT INTO source_cursor(source, cursor_key, updated_at) VALUES ('codex', ?1, ?2)",
        )?;
        for index in 0..500 {
            insert_cursor.execute(rusqlite::params![format!("cursor-{index}"), &now])?;
        }
        drop(insert_cursor);
        drop(conn);

        let addr = serve(store, Some(0)).await?;
        let (status, payload) = route_json(
            addr,
            "GET",
            "/api/dashboard?scope=interactive&range=7d&window=week&source=codex&timezone=UTC",
            None,
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload["overview"]["total"]["total_tokens"], 10);
        assert_eq!(payload["health"]["cursor_count"], 500);
        assert!(payload["health"]["cursors"].is_null());
        assert!(payload["trends"].as_array().is_some());
        assert!(payload["day_trends"].is_null());
        assert!(payload["week_trends"].is_null());
        assert!(payload["activity"].is_null());
        assert!(payload["explorer"].is_null());
        assert!(
            serde_json::to_vec(&payload)?.len() <= 128 * 1024,
            "interactive payload must stay within the accepted 128 KiB boundary"
        );
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
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start,
                input_tokens, cache_creation_tokens, cache_read_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                event_count, updated_at
            ) VALUES (
                'codex', 'gpt-5', '2026-05-01T00:00:00Z',
                10, 0, 0, 0, 0, 10, 1, '2026-05-01T00:00:00Z'
            )
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
    async fn api_dashboard_embeds_sync_command_center_contract() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO source_sync_status(
                source, files_processed, changed_files, bytes_scanned,
                events_seen, events_replayed, events_inserted, stored_events,
                parse_ms, write_ms, lock_wait_ms, updated_at
            ) VALUES
                ('codex', 2, 1, 2048, 7, 0, 5, 42, 10, 4, 0, '2026-05-29T00:01:00Z'),
                ('claude', 1, 0, 1024, 0, 0, 0, 11, 3, 1, 0, '2026-05-29T00:02:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO source_file(source, file_path, state, last_seen_at, last_state_change_at)
            VALUES ('codex', ?1, 'missing', NULL, '2026-05-29T00:00:00Z')
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
            ) VALUES ('codex:event:sync-center', 'codex', 'gpt-5',
                '2026-05-29T00:00:00Z', '2026-05-29T00:00:00Z',
                10, 0, 0, 0, 0, 10,
                'project-a', 'Project A', NULL, 'path-a',
                NULL, NULL, NULL, '2026-05-29T00:00:00Z',
                0.1, 0.1, 'static', 'static-v1', NULL)
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO usage_bucket_30m(
                source, model, hour_start,
                input_tokens, cache_creation_tokens, cache_read_tokens,
                output_tokens, reasoning_output_tokens, total_tokens,
                event_count, updated_at
            ) VALUES (
                'codex', 'gpt-5', '2026-05-29T00:00:00Z',
                10, 0, 0, 0, 0, 10, 1, '2026-05-29T00:00:00Z'
            )
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO run_log(command, status, summary, error, started_at, finished_at, duration_ms)
            VALUES ('sync', 'success', 'human summary that must not be parsed', NULL,
                    '2026-05-29T00:00:00Z', '2026-05-29T00:03:00Z', 180000)
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO run_log(command, status, summary, error, started_at, finished_at, duration_ms)
            VALUES ('sync', 'failed', 'failed human summary that must not be parsed', ?1,
                    '2026-05-29T00:04:00Z', '2026-05-29T00:05:00Z', 60000)
            "#,
            [r"failed while reading D:\Users\alice\.llmusage\secret.jsonl: raw token abc123"],
        )?;
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

        let center = &payload["sync_command_center"];
        assert_eq!(center["mode"], "live");
        assert_eq!(center["tone"], "warn");
        assert_eq!(center["safety"]["ordinary_sync_safe"], true);
        assert_eq!(center["safety"]["lossy_rebuild_risk"], true);
        assert_eq!(center["safety"]["risk_sources"][0], "codex");
        assert_eq!(center["last_run"]["status"], "failed");
        assert_eq!(center["last_run"]["finished_at"], "2026-05-29T00:05:00Z");
        assert_eq!(
            center["last_run"]["error_key"],
            "syncCenter.reason.lastRunFailed"
        );
        assert!(center["last_run"].get("error").is_none());
        assert!(center["last_run"].get("summary").is_none());
        let last_run_text = serde_json::to_string(&center["last_run"])?;
        assert!(!last_run_text.contains("D:\\Users\\alice"));
        assert!(!last_run_text.contains("secret.jsonl"));
        assert!(!last_run_text.contains("abc123"));
        assert_eq!(center["metrics"]["inserted_delta"], 5);
        assert_eq!(center["metrics"]["stored_events"], 42);
        assert_eq!(center["sources"][0]["source"], "codex");
        assert_eq!(center["sources"][0]["events_inserted"], 5);
        assert!(center["sources"][0]["share"].as_f64().unwrap() > 0.0);
        assert!(center["sources"][0].get("last_error").is_none());
        Ok(())
    }

    #[tokio::test]
    async fn api_dashboard_sync_command_center_filters_rebuild_risk_by_source() -> anyhow::Result<()>
    {
        let (_temp, store) = make_store()?;
        let conn = store.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO source_sync_status(
                source, files_processed, changed_files, bytes_scanned,
                events_seen, events_replayed, events_inserted, stored_events,
                parse_ms, write_ms, lock_wait_ms, updated_at
            ) VALUES
                ('codex', 1, 1, 2048, 3, 0, 3, 30, 10, 4, 0, '2026-05-29T00:01:00Z'),
                ('claude', 1, 0, 1024, 1, 0, 1, 10, 3, 1, 0, '2026-05-29T00:02:00Z')
            "#,
            [],
        )?;
        conn.execute(
            r#"
            INSERT INTO source_file(source, file_path, state, last_seen_at, last_state_change_at)
            VALUES ('claude', ?1, 'missing', NULL, '2026-05-29T00:00:00Z')
            "#,
            [r"D:\missing\claude.jsonl"],
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
            ) VALUES
                ('codex:event:sync-center-filter', 'codex', 'gpt-5',
                    '2026-05-29T00:00:00Z', '2026-05-29T00:00:00Z',
                    10, 0, 0, 0, 0, 10,
                    'project-a', 'Project A', NULL, 'path-a',
                    NULL, NULL, NULL, '2026-05-29T00:00:00Z',
                    0.1, 0.1, 'static', 'static-v1', NULL),
                ('claude:event:sync-center-filter-risk', 'claude', 'claude-sonnet',
                    '2026-05-29T00:00:00Z', '2026-05-29T00:00:00Z',
                    20, 0, 0, 0, 0, 20,
                    'project-b', 'Project B', NULL, 'path-b',
                    NULL, NULL, NULL, '2026-05-29T00:00:00Z',
                    0.2, 0.2, 'static', 'static-v1', NULL)
            "#,
            [],
        )?;
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

        let center = &payload["sync_command_center"];
        assert_eq!(center["safety"]["lossy_rebuild_risk"], false);
        assert!(
            center["safety"]["risk_sources"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let sources = center["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["source"], "codex");
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

    #[tokio::test]
    async fn asset_response_sets_etag_and_revalidates() -> anyhow::Result<()> {
        let asset = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset");

        let response = asset.as_response(&HeaderMap::new());
        assert_eq!(response.status(), StatusCode::OK);
        let etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
            .expect("ETag header")
            .to_string();
        assert!(etag.starts_with('"') && etag.ends_with('"'));
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some(asset.content_type)
        );
        let body = to_bytes(response.into_body(), usize::MAX).await?;
        assert_eq!(body.as_ref(), asset.body.as_bytes());

        // ETag 在进程内稳定，且不同资源互不相同。
        let second = asset.as_response(&HeaderMap::new());
        assert_eq!(
            second
                .headers()
                .get(header::ETAG)
                .and_then(|value| value.to_str().ok()),
            Some(etag.as_str())
        );
        let other = asset_manifest()
            .iter()
            .find(|asset| asset.path == "components.css")
            .expect("components.css asset")
            .as_response(&HeaderMap::new());
        assert_ne!(
            other
                .headers()
                .get(header::ETAG)
                .and_then(|value| value.to_str().ok()),
            Some(etag.as_str())
        );

        // If-None-Match 命中（精确 / 弱校验 / 列表）返回 304 空 body。
        for candidate in [
            etag.clone(),
            format!("W/{etag}"),
            format!("\"0000000000000000\", {etag}"),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::IF_NONE_MATCH, HeaderValue::from_str(&candidate)?);
            let response = asset.as_response(&headers);
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{candidate}");
            assert_eq!(
                response
                    .headers()
                    .get(header::ETAG)
                    .and_then(|value| value.to_str().ok()),
                Some(etag.as_str())
            );
            assert_eq!(
                response
                    .headers()
                    .get(header::CACHE_CONTROL)
                    .and_then(|value| value.to_str().ok()),
                Some("no-cache")
            );
            let body = to_bytes(response.into_body(), usize::MAX).await?;
            assert!(body.is_empty(), "{candidate}");
        }

        // 未命中时回退到 200 全量响应。
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("\"0000000000000000\""),
        );
        let response = asset.as_response(&headers);
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await?;
        assert_eq!(body.as_ref(), asset.body.as_bytes());
        Ok(())
    }

    #[tokio::test]
    async fn served_assets_support_etag_revalidation() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;

        let (status, head, body) = route_bytes(addr, "/assets/app.js", &[]).await?;
        assert_eq!(status, StatusCode::OK);
        let etag = response_header(&head, "etag")
            .expect("ETag header")
            .to_string();
        assert_eq!(response_header(&head, "cache-control"), Some("no-cache"));
        let expected = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;
        assert_eq!(body, expected.as_bytes());

        let (status, head, body) =
            route_bytes(addr, "/assets/app.js", &[("If-None-Match", &etag)]).await?;
        assert_eq!(status, StatusCode::NOT_MODIFIED);
        assert_eq!(response_header(&head, "etag"), Some(etag.as_str()));
        assert!(body.is_empty());

        let (status, _, body) = route_bytes(
            addr,
            "/assets/app.js",
            &[("If-None-Match", "\"0000000000000000\"")],
        )
        .await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, expected.as_bytes());
        Ok(())
    }

    #[tokio::test]
    async fn render_key_asset_is_registered_and_served_without_blocked_alias() -> anyhow::Result<()>
    {
        let expected = asset_manifest()
            .iter()
            .find(|asset| asset.path == "data/render-key.js")
            .expect("render-key.js asset")
            .body;
        assert!(
            asset_manifest()
                .iter()
                .all(|asset| !asset.path.contains("fingerprint"))
        );

        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;
        let (status, _, body) = route_bytes(addr, "/assets/data/render-key.js", &[]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, expected.as_bytes());

        let (status, _, _) = route_bytes(addr, "/assets/data/fingerprint.js", &[]).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        Ok(())
    }

    #[tokio::test]
    async fn served_assets_and_api_negotiate_compression() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;

        let app_js = asset_manifest()
            .iter()
            .find(|asset| asset.path == "app.js")
            .expect("app.js asset")
            .body;

        // 不带 Accept-Encoding：原文直出，无 Content-Encoding。
        let (status, head, identity) = route_bytes(addr, "/assets/app.js", &[]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_header(&head, "content-encoding"), None);
        assert_eq!(identity, app_js.as_bytes());

        // gzip 协商：带 Content-Encoding，gzip magic，且体积更小。
        let (status, head, gzipped) =
            route_bytes(addr, "/assets/app.js", &[("Accept-Encoding", "gzip")]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_header(&head, "content-encoding"), Some("gzip"));
        assert_eq!(&gzipped[..2], &[0x1f, 0x8b]);
        assert!(gzipped.len() < identity.len());

        // br 协商同理（brotli 无 magic，只校验协商与体积）。
        let (status, head, brotli) =
            route_bytes(addr, "/assets/app.js", &[("Accept-Encoding", "br")]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_header(&head, "content-encoding"), Some("br"));
        assert!(brotli.len() < identity.len());

        // 客户端显式拒绝 gzip 时保持 identity。
        let (status, head, plain) =
            route_bytes(addr, "/assets/app.js", &[("Accept-Encoding", "gzip;q=0")]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_header(&head, "content-encoding"), None);
        assert_eq!(plain, app_js.as_bytes());

        // JSON API 同样参与压缩协商，但不新增缓存头。
        let (status, head, _) =
            route_bytes(addr, "/api/dashboard", &[("Accept-Encoding", "gzip")]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_header(&head, "content-encoding"), Some("gzip"));
        let (status, head, _) = route_bytes(addr, "/api/dashboard", &[]).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response_header(&head, "content-encoding"), None);
        assert_eq!(response_header(&head, "cache-control"), None);
        assert_eq!(response_header(&head, "etag"), None);
        Ok(())
    }

    #[tokio::test]
    async fn served_index_body_is_stable_across_requests() -> anyhow::Result<()> {
        let (_temp, store) = make_store()?;
        let addr = serve(store, Some(0)).await?;

        let (first_status, first) = route_text(addr, "GET", "/").await?;
        let (second_status, second) = route_text(addr, "GET", "/").await?;
        assert_eq!(first_status, StatusCode::OK);
        assert_eq!(second_status, StatusCode::OK);
        assert_eq!(first, second);
        assert!(first.contains("data-mode=\"live\""));
        Ok(())
    }

    /// Times one full dashboard request with generous measurement timeouts.
    async fn timed_full_request(addr: SocketAddr) -> anyhow::Result<(StatusCode, Duration)> {
        let started = Instant::now();
        let raw = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr)?;
            stream.set_read_timeout(Some(Duration::from_secs(60)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            stream.write_all(
                format!(
                    "GET /api/dashboard?scope=full HTTP/1.1\r\n\
                     Host: {addr}\r\n\
                     Accept: application/json\r\n\
                     Connection: close\r\n\r\n"
                )
                .as_bytes(),
            )?;
            let mut raw = String::new();
            stream.read_to_string(&mut raw)?;
            Ok::<_, anyhow::Error>(raw)
        })
        .await??;
        let elapsed = started.elapsed();
        let status_code = raw
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| anyhow::anyhow!("invalid response: {raw:?}"))?;
        Ok((StatusCode::from_u16(status_code)?, elapsed))
    }

    /// Measurement-only baseline for two concurrent full dashboard requests.
    /// Run explicitly: `cargo test --lib measure_stress_double_full_concurrency -- --ignored --nocapture --test-threads=1`
    #[tokio::test]
    #[ignore = "measurement test; run explicitly for baseline/after reports"]
    async fn measure_stress_double_full_concurrency() -> anyhow::Result<()> {
        let fixture = crate::testing::Fixture::new()?;
        fixture.seed_stress_dashboard(4_000, 1_000, 25)?;
        let addr = serve(fixture.store().clone(), Some(0)).await?;

        let (single_status, single_elapsed) = timed_full_request(addr).await?;
        eprintln!("single full status={single_status} elapsed={single_elapsed:?}");
        assert_eq!(single_status, StatusCode::OK);

        let pair_started = Instant::now();
        let (first, second) = tokio::join!(timed_full_request(addr), timed_full_request(addr));
        let pair_wall = pair_started.elapsed();
        let (first_status, first_elapsed) = first?;
        let (second_status, second_elapsed) = second?;
        eprintln!("concurrent full A status={first_status} elapsed={first_elapsed:?}");
        eprintln!("concurrent full B status={second_status} elapsed={second_elapsed:?}");
        eprintln!("concurrent pair wall={pair_wall:?}");
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
