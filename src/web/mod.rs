use std::{collections::HashMap, net::SocketAddr};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use serde_json::json;
use tokio::net::TcpListener;
use tracing::{error, info};

use crate::{query::Dashboard, store::Store};

mod assets;
mod shell;

#[derive(Clone)]
pub struct WebState {
    pub store: Store,
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
    let state = WebState { store };
    let app = Router::new()
        .route("/", get(index_live))
        .route("/assets/{*path}", get(asset_file))
        .route("/api/overview", get(api_overview))
        .route("/api/trends", get(api_trends))
        .route("/api/models", get(api_models))
        .route("/api/sources", get(api_sources))
        .route("/api/projects", get(api_projects))
        .route("/api/costs", get(api_costs))
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
        load_via_dashboard(&state, |d| d.overview()),
    )
}

async fn api_trends(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let window = params.get("window").map(String::as_str).unwrap_or("day");
    api_json(
        "/api/trends",
        load_via_dashboard(&state, |d| d.trends(window)),
    )
}

async fn api_models(State(state): State<WebState>) -> Response {
    api_json(
        "/api/models",
        load_via_dashboard(&state, |d| d.model_breakdown()),
    )
}

async fn api_sources(State(state): State<WebState>) -> Response {
    api_json(
        "/api/sources",
        load_via_dashboard(&state, |d| d.source_breakdown()),
    )
}

async fn api_projects(State(state): State<WebState>) -> Response {
    api_json(
        "/api/projects",
        load_via_dashboard(&state, |d| d.project_breakdown()),
    )
}

async fn api_costs(State(state): State<WebState>) -> Response {
    api_json(
        "/api/costs",
        load_via_dashboard(&state, |d| d.cost_breakdown()),
    )
}

async fn api_health(State(state): State<WebState>) -> Response {
    api_json("/api/health", load_via_dashboard(&state, |d| d.health()))
}

fn load_via_dashboard<T>(state: &WebState, f: impl FnOnce(&Dashboard) -> Result<T>) -> Result<T> {
    let dashboard = Dashboard::open(&state.store)?;
    f(&dashboard)
}

fn api_json<T>(endpoint: &'static str, result: Result<T>) -> Response
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

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use axum::{body::to_bytes, http::StatusCode};

    use super::{api_json, asset_manifest, live_index_html, snapshot_index_html};

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
        assert!(html.contains("llmusage 本地用量概览"));
        assert!(html.contains("用量趋势"));
        assert!(html.contains("模型用量分布"));
        assert!(html.contains("来源分布"));
        assert!(html.contains("项目排行"));
        assert!(html.contains("运行状态"));
        assert!(!html.contains("llmusage 本地账本"));
        assert!(!html.contains("趋势聚焦"));
        assert!(!html.contains("模型偏好"));
        assert!(!html.contains("来源节奏"));
        assert!(!html.contains("项目热区"));
        assert!(!html.contains("运行脉冲"));
    }

    #[test]
    fn snapshot_shell_uses_snapshot_mode_marker() {
        let html = snapshot_index_html();
        assert!(html.contains("data-mode=\"snapshot\""));
        assert!(html.contains("离线文件 · 静态导出"));
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
                "data.js",
                "data/fetch.js",
                "data/format.js",
                "data/derive.js",
                "render.js",
                "render/hero.js",
                "render/charts.js",
                "render/tables.js",
                "render/health.js",
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
        assert!(app_js.contains("detail.textContent = message;"));
        assert!(!app_js.contains("empty-state mono\">${String(error"));
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
    fn api_error_payload_is_structured_json() {
        let response = api_json::<serde_json::Value>("/api/test", Err(anyhow!("boom")));
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
        assert_eq!(payload["error"]["detail"], "boom");
    }
}
