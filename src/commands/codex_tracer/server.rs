//! Web server for codex-tracer dashboard.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tracing::{error, info};

use super::parser::parse_codex_jsonl_for_tracer;
use super::store::{CallFilters, CodexTracerStore};

#[derive(Clone)]
struct ServerState {
    store: Arc<Mutex<CodexTracerStore>>,
}

/// Serve the codex-tracer dashboard.
pub async fn serve_dashboard(db_path: PathBuf, port: u16, open_browser: bool) -> Result<()> {
    let store = CodexTracerStore::open(&db_path).context("Failed to open codex-tracer database")?;

    let state = ServerState {
        store: Arc::new(Mutex::new(store)),
    };

    let app = Router::new()
        .route("/", get(handle_index))
        .route("/api/calls", get(handle_calls_query))
        .route("/api/refresh", get(handle_refresh))
        .route("/api/stats", get(handle_stats))
        .route("/dashboard.html", get(handle_index))
        .route("/dashboard.js", get(serve_js_asset::<DashboardJs>))
        .route(
            "/dashboard_actions.js",
            get(serve_js_asset::<DashboardActionsJs>),
        )
        .route(
            "/dashboard_analysis.js",
            get(serve_js_asset::<DashboardAnalysisJs>),
        )
        .route(
            "/dashboard_call_diagnostics.js",
            get(serve_js_asset::<DashboardCallDiagnosticsJs>),
        )
        .route(
            "/dashboard_call_investigator.js",
            get(serve_js_asset::<DashboardCallInvestigatorJs>),
        )
        .route(
            "/dashboard_cells.js",
            get(serve_js_asset::<DashboardCellsJs>),
        )
        .route("/dashboard_data.js", get(serve_js_asset::<DashboardDataJs>))
        .route(
            "/dashboard_details.js",
            get(serve_js_asset::<DashboardDetailsJs>),
        )
        .route(
            "/dashboard_events.js",
            get(serve_js_asset::<DashboardEventsJs>),
        )
        .route(
            "/dashboard_filters.js",
            get(serve_js_asset::<DashboardFiltersJs>),
        )
        .route(
            "/dashboard_format.js",
            get(serve_js_asset::<DashboardFormatJs>),
        )
        .route(
            "/dashboard_insights.js",
            get(serve_js_asset::<DashboardInsightsJs>),
        )
        .route("/dashboard_i18n.js", get(serve_js_asset::<DashboardI18nJs>))
        .route("/dashboard_live.js", get(serve_js_asset::<DashboardLiveJs>))
        .route(
            "/dashboard_payload_cache.js",
            get(serve_js_asset::<DashboardPayloadCacheJs>),
        )
        .route(
            "/dashboard_state.js",
            get(serve_js_asset::<DashboardStateJs>),
        )
        .route(
            "/dashboard_status.js",
            get(serve_js_asset::<DashboardStatusJs>),
        )
        .route(
            "/dashboard_tables.js",
            get(serve_js_asset::<DashboardTablesJs>),
        )
        .route(
            "/dashboard_tooltips.js",
            get(serve_js_asset::<DashboardTooltipsJs>),
        )
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {}", addr))?;

    let actual_addr = listener.local_addr()?;
    let dashboard_url = format!("http://{}", actual_addr);

    println!("🚀 Codex Tracer Dashboard: {}", dashboard_url);
    info!("Codex Tracer dashboard listening on {}", actual_addr);

    if open_browser && open_browser_to_url(&dashboard_url).is_err() {
        println!(
            "Failed to open browser automatically. Please open manually: {}",
            dashboard_url
        );
    }

    axum::serve(listener, app).await.context("Server error")?;

    Ok(())
}

async fn handle_index(State(state): State<ServerState>) -> Response {
    let store = state.store.lock().await;

    // Query all events
    let calls = match store.query_calls(&CallFilters {
        limit: Some(10000),
        ..Default::default()
    }) {
        Ok(calls) => calls,
        Err(err) => {
            error!(error = %err, "Failed to query calls");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query calls: {}", err),
            )
                .into_response();
        }
    };

    // Generate payload
    let payload = json!({
        "calls": calls,
        "metadata": {
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "schema": "codex-tracer-v1",
            "total_events": calls.len(),
        }
    });

    let payload_json = match serde_json::to_string(&payload) {
        Ok(json) => json,
        Err(err) => {
            error!(error = %err, "Failed to serialize payload");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to serialize payload: {}", err),
            )
                .into_response();
        }
    };

    // Load template and inject data
    let template = include_str!("dashboard/dashboard_template.html");
    let css = include_str!("dashboard/dashboard.css");

    let html = template
        .replace("__HTML_LANG__", "en")
        .replace("__HTML_DIR__", "ltr")
        .replace("__TITLE__", "Codex Tracer Dashboard")
        .replace("__STYLESHEET_LINKS__", &format!("<style>{}</style>", css))
        .replace(
            "__BODY_ATTRS__",
            &format!(
                " data-dashboard-payload='{}'",
                escape_html_attr(&payload_json)
            ),
        )
        .replace("__GUIDE_LINK__", "");

    Html(html).into_response()
}

#[derive(Debug, Deserialize)]
struct CallsQueryParams {
    model: Option<String>,
    since: Option<String>,
    until: Option<String>,
    include_archived: Option<bool>,
    limit: Option<i64>,
}

async fn handle_calls_query(
    State(state): State<ServerState>,
    Query(params): Query<CallsQueryParams>,
) -> Response {
    let store = state.store.lock().await;

    let filters = CallFilters {
        model: params.model,
        since: params.since,
        until: params.until,
        include_archived: params.include_archived.unwrap_or(false),
        limit: params.limit.and_then(|v| usize::try_from(v).ok()),
    };

    match store.query_calls(&filters) {
        Ok(calls) => Json(json!({
            "calls": calls,
            "count": calls.len(),
        }))
        .into_response(),
        Err(err) => {
            error!(error = %err, "Failed to query calls");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "code": "internal_error",
                        "message": "Failed to query calls",
                        "detail": err.to_string(),
                    }
                })),
            )
                .into_response()
        }
    }
}

async fn handle_refresh(State(state): State<ServerState>) -> Response {
    info!("Refresh requested");

    // Determine Codex rollout directory
    let codex_home = match std::env::var("CODEX_HOME") {
        Ok(path) => PathBuf::from(path),
        Err(_) => {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".codex")
        }
    };

    let rollout_dir = codex_home.join("rollout");

    if !rollout_dir.exists() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": {
                    "code": "codex_not_found",
                    "message": "Codex rollout directory not found",
                    "detail": format!("Expected directory: {}", rollout_dir.display()),
                }
            })),
        )
            .into_response();
    }

    // Parse all JSONL files
    let mut all_events = Vec::new();
    let mut file_count = 0;
    let mut error_count = 0;

    for entry in walkdir::WalkDir::new(&rollout_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }

        file_count += 1;
        match parse_codex_jsonl_for_tracer(path) {
            Ok(events) => {
                all_events.extend(events);
            }
            Err(err) => {
                error!(file = %path.display(), error = %err, "Failed to parse JSONL file");
                error_count += 1;
            }
        }
    }

    // Insert events into database
    let mut store = state.store.lock().await;
    let inserted = match store.upsert_events(&all_events) {
        Ok(count) => count,
        Err(err) => {
            error!(error = %err, "Failed to insert events");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "code": "internal_error",
                        "message": "Failed to insert events",
                        "detail": err.to_string(),
                    }
                })),
            )
                .into_response();
        }
    };

    info!(
        files = file_count,
        events = all_events.len(),
        inserted = inserted,
        errors = error_count,
        "Refresh completed"
    );

    Json(json!({
        "ok": true,
        "files_parsed": file_count,
        "events_found": all_events.len(),
        "events_inserted": inserted,
        "errors": error_count,
    }))
    .into_response()
}

async fn handle_stats(State(state): State<ServerState>) -> Response {
    let store = state.store.lock().await;

    match store.count_events() {
        Ok(count) => Json(json!({
            "total_events": count,
        }))
        .into_response(),
        Err(err) => {
            error!(error = %err, "Failed to count events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": {
                        "code": "internal_error",
                        "message": "Failed to get stats",
                        "detail": err.to_string(),
                    }
                })),
            )
                .into_response()
        }
    }
}

fn escape_html_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn open_browser_to_url(url: &str) -> Result<()> {
    use std::process::{Command, Stdio};

    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    } else {
        Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
    }?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("Browser launcher exited with status {}", status)
    }
}

// Marker types for JavaScript assets
struct DashboardJs;
struct DashboardActionsJs;
struct DashboardAnalysisJs;
struct DashboardCallDiagnosticsJs;
struct DashboardCallInvestigatorJs;
struct DashboardCellsJs;
struct DashboardDataJs;
struct DashboardDetailsJs;
struct DashboardEventsJs;
struct DashboardFiltersJs;
struct DashboardFormatJs;
struct DashboardInsightsJs;
struct DashboardI18nJs;
struct DashboardLiveJs;
struct DashboardPayloadCacheJs;
struct DashboardStateJs;
struct DashboardStatusJs;
struct DashboardTablesJs;
struct DashboardTooltipsJs;

// Trait for serving JS assets
trait JsAsset {
    fn content() -> &'static str;
}

macro_rules! impl_js_asset {
    ($type:ty, $path:literal) => {
        impl JsAsset for $type {
            fn content() -> &'static str {
                include_str!($path)
            }
        }
    };
}

impl_js_asset!(DashboardJs, "dashboard/dashboard.js");
impl_js_asset!(DashboardActionsJs, "dashboard/dashboard_actions.js");
impl_js_asset!(DashboardAnalysisJs, "dashboard/dashboard_analysis.js");
impl_js_asset!(
    DashboardCallDiagnosticsJs,
    "dashboard/dashboard_call_diagnostics.js"
);
impl_js_asset!(
    DashboardCallInvestigatorJs,
    "dashboard/dashboard_call_investigator.js"
);
impl_js_asset!(DashboardCellsJs, "dashboard/dashboard_cells.js");
impl_js_asset!(DashboardDataJs, "dashboard/dashboard_data.js");
impl_js_asset!(DashboardDetailsJs, "dashboard/dashboard_details.js");
impl_js_asset!(DashboardEventsJs, "dashboard/dashboard_events.js");
impl_js_asset!(DashboardFiltersJs, "dashboard/dashboard_filters.js");
impl_js_asset!(DashboardFormatJs, "dashboard/dashboard_format.js");
impl_js_asset!(DashboardInsightsJs, "dashboard/dashboard_insights.js");
impl_js_asset!(DashboardI18nJs, "dashboard/dashboard_i18n.js");
impl_js_asset!(DashboardLiveJs, "dashboard/dashboard_live.js");
impl_js_asset!(
    DashboardPayloadCacheJs,
    "dashboard/dashboard_payload_cache.js"
);
impl_js_asset!(DashboardStateJs, "dashboard/dashboard_state.js");
impl_js_asset!(DashboardStatusJs, "dashboard/dashboard_status.js");
impl_js_asset!(DashboardTablesJs, "dashboard/dashboard_tables.js");
impl_js_asset!(DashboardTooltipsJs, "dashboard/dashboard_tooltips.js");

async fn serve_js_asset<T: JsAsset>() -> Response {
    (
        StatusCode::OK,
        [("Content-Type", "application/javascript; charset=utf-8")],
        T::content(),
    )
        .into_response()
}
