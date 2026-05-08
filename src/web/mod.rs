use std::{collections::HashMap, net::SocketAddr};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::{
    error::Result as LlmusageResult,
    models::SourceKind,
    query::{Dashboard, LogsQuery, QueryFilter},
    store::Store,
    sync::{JobRegistry, SyncOptions},
};

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
        .route("/api/overview", get(api_overview))
        .route("/api/trends", get(api_trends))
        .route("/api/models", get(api_models))
        .route("/api/sources", get(api_sources))
        .route("/api/projects", get(api_projects))
        .route("/api/costs", get(api_costs))
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

async fn api_overview(State(state): State<WebState>) -> Response {
    api_json(
        "/api/overview",
        load_via_dashboard(&state, |d| d.overview(&Default::default())),
    )
}

async fn api_trends(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let window = params.get("window").map(String::as_str).unwrap_or("day");
    api_json(
        "/api/trends",
        load_via_dashboard(&state, |d| {
            #[allow(deprecated)]
            d.trends(window, &Default::default())
        }),
    )
}

async fn api_models(State(state): State<WebState>) -> Response {
    api_json(
        "/api/models",
        load_via_dashboard(&state, |d| d.model_breakdown(&Default::default())),
    )
}

async fn api_sources(State(state): State<WebState>) -> Response {
    api_json(
        "/api/sources",
        load_via_dashboard(&state, |d| d.source_breakdown(&Default::default())),
    )
}

async fn api_projects(State(state): State<WebState>) -> Response {
    api_json(
        "/api/projects",
        load_via_dashboard(&state, |d| d.project_breakdown(&Default::default())),
    )
}

async fn api_costs(State(state): State<WebState>) -> Response {
    api_json(
        "/api/costs",
        load_via_dashboard(&state, |d| d.cost_breakdown(&Default::default())),
    )
}

async fn api_home_overview(State(state): State<WebState>) -> Response {
    api_json(
        "/api/home_overview",
        load_via_dashboard(&state, |d| d.home_overview(&Default::default())),
    )
}

async fn api_heatmap(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let days = params
        .get("days")
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(365);
    let source = params.get("source").and_then(|raw| match raw.as_str() {
        "codex" => Some(crate::models::SourceKind::Codex),
        "claude" => Some(crate::models::SourceKind::Claude),
        "opencode" => Some(crate::models::SourceKind::Opencode),
        _ => None,
    });
    let filter = crate::query::QueryFilter {
        source,
        ..Default::default()
    };
    api_json(
        "/api/heatmap",
        load_via_dashboard(&state, |d| d.heatmap(&filter, days)),
    )
}

async fn api_logs(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let source = params
        .get("source")
        .and_then(|raw| SourceKind::parse_id(raw.trim()));
    let model = params
        .get("model")
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty());
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

    let filter = QueryFilter {
        source,
        model,
        since: params
            .get("since")
            .and_then(|raw| chrono::NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()),
        until: params
            .get("until")
            .and_then(|raw| chrono::NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d").ok()),
        ..Default::default()
    };
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

    api_json("/api/logs", load_via_dashboard(&state, |d| d.logs(&query)))
}

async fn api_health(State(state): State<WebState>) -> Response {
    api_json("/api/health", load_via_dashboard(&state, |d| d.health()))
}

async fn api_diagnostics(State(state): State<WebState>) -> Response {
    api_json(
        "/api/diagnostics",
        load_via_dashboard(&state, |d| d.diagnostics()),
    )
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

fn load_via_dashboard<T>(
    state: &WebState,
    f: impl FnOnce(&Dashboard) -> LlmusageResult<T>,
) -> LlmusageResult<T> {
    let dashboard = Dashboard::open(&state.store)?;
    f(&dashboard)
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
        time::Duration,
    };

    use axum::{body::to_bytes, http::StatusCode};
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
        assert!(html.contains("id=\"cost\""));
        assert!(html.contains("id=\"status\""));
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
                "render/costs.js",
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
        assert!(app_js.contains("loadSection(state, 'overview', '/api/overview')"));
        assert!(app_js.contains("loadTrendWindow(state, state.trendWindow)"));
        assert!(!app_js.contains("window.LLMUSAGE_DATA"));
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
            "data-i18n=\"shell.nav.item.cost\"",
            "data-i18n=\"shell.btn.export\"",
            "data-i18n=\"shell.btn.sync\"",
            "data-i18n=\"shell.endpoint.lastSync\"",
            "data-i18n=\"shell.crumb.local\"",
            "data-i18n=\"shell.tag.local\"",
            "data-i18n-html=\"shell.hero.title.html\"",
        ] {
            assert!(html.contains(key), "missing i18n key in shell HTML: {key}");
        }
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
        assert!(app_js.contains("initTheme()"));
        assert!(app_js.contains("applyDomI18n(document)"));
        assert!(app_js.contains("onLocaleChange"));
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
