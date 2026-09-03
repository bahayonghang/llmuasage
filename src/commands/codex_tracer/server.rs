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
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::ingest::{CodexTracerIngestOptions, ingest_rollout_dir};
use super::store::{CallFilters, CodexTracerStore, INDEX_QUERY_LIMIT, LIST_QUERY_LIMIT_MAX};

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

    let app = dashboard_router(state);

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

fn dashboard_router(state: ServerState) -> Router {
    Router::new()
        .route("/", get(handle_index))
        .route("/api/calls", get(handle_calls_query))
        .route("/api/refresh", post(handle_refresh))
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
        .with_state(state)
}

fn json_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (
        status,
        Json(json!({
            "error": {
                "code": code,
                "message": message,
            }
        })),
    )
        .into_response()
}

fn html_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (status, format!("{code}: {message}")).into_response()
}

fn clamp_list_query_limit(limit: Option<i64>) -> usize {
    match limit {
        Some(value) => usize::try_from(value)
            .ok()
            .filter(|value| *value > 0)
            .map(|value| value.min(LIST_QUERY_LIMIT_MAX))
            .unwrap_or(LIST_QUERY_LIMIT_MAX),
        None => LIST_QUERY_LIMIT_MAX,
    }
}

async fn handle_index(State(state): State<ServerState>) -> Response {
    let store = state.store.lock().await;

    let calls = match store.query_calls(&CallFilters {
        limit: Some(INDEX_QUERY_LIMIT),
        ..Default::default()
    }) {
        Ok(calls) => calls,
        Err(err) => {
            error!(error = %err, "Failed to query calls");
            return html_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to query calls",
            );
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
            return html_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to serialize payload",
            );
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
        limit: Some(clamp_list_query_limit(params.limit)),
    };

    match store.query_calls(&filters) {
        Ok(calls) => Json(json!({
            "calls": calls,
            "count": calls.len(),
        }))
        .into_response(),
        Err(err) => {
            error!(error = %err, "Failed to query calls");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to query calls",
            )
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
        error!(path = %rollout_dir.display(), "Codex rollout directory not found");
        return json_error(
            StatusCode::NOT_FOUND,
            "codex_not_found",
            "Codex rollout directory not found",
        );
    }

    let mut store = state.store.lock().await;
    let stats = match ingest_rollout_dir(
        &mut store,
        &rollout_dir,
        &CancellationToken::new(),
        CodexTracerIngestOptions::default(),
    ) {
        Ok(stats) => stats,
        Err(err) => {
            error!(error = %err, "Failed to refresh Codex tracer events");
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to refresh events",
            );
        }
    };

    info!(
        files = stats.files_seen,
        records = stats.records_read,
        events = stats.events_found,
        inserted = stats.rows_written,
        batch_peak = stats.batch_peak,
        errors = stats.errors,
        "Refresh completed"
    );

    Json(json!({
        "ok": true,
        "files_parsed": stats.files_seen,
        "events_found": stats.events_found,
        "events_inserted": stats.rows_written,
        "errors": stats.errors,
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
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to get stats",
            )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::codex_tracer::models::CodexTracerEvent;

    use std::ffi::OsStr;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpStream};
    use std::path::Path;
    use std::time::Duration;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    struct EnvGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(value) = &self.previous {
                    std::env::set_var(self.key, value);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    async fn serve_test_app(store: CodexTracerStore) -> SocketAddr {
        let app = dashboard_router(ServerState {
            store: Arc::new(Mutex::new(store)),
        });
        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("bind tracer test listener");
        let addr = listener.local_addr().expect("local addr");
        assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("tracer test server");
        });
        addr
    }

    async fn http_request(addr: SocketAddr, method: &str, path: &str) -> (StatusCode, String) {
        let method = method.to_string();
        let path = path.to_string();
        let raw = tokio::task::spawn_blocking(move || {
            let mut stream = None;
            for attempt in 0..50 {
                match TcpStream::connect(addr) {
                    Ok(connected) => {
                        stream = Some(connected);
                        break;
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::ConnectionRefused && attempt < 49 =>
                    {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(err) => return Err(err),
                }
            }
            let mut stream = stream.expect("connect tracer test listener");
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
            let mut raw = Vec::new();
            stream.read_to_end(&mut raw)?;
            Ok::<_, std::io::Error>(raw)
        })
        .await
        .expect("http join")
        .expect("http request");

        let split = raw
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("invalid HTTP response");
        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
        let status_code = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|raw| raw.parse::<u16>().ok())
            .expect("invalid status line");
        let body = decode_http_body(&head, &raw[split + 4..]);
        (StatusCode::from_u16(status_code).expect("status"), body)
    }

    fn decode_http_body(head: &str, body: &[u8]) -> String {
        let bytes = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            let mut rest = body;
            let mut decoded = Vec::new();
            while let Some(line_end) = rest.windows(2).position(|window| window == b"\r\n") {
                let Ok(size_text) = std::str::from_utf8(&rest[..line_end]) else {
                    break;
                };
                let Ok(size) = usize::from_str_radix(size_text.trim(), 16) else {
                    break;
                };
                rest = &rest[line_end + 2..];
                if size == 0 {
                    break;
                }
                if rest.len() < size + 2 {
                    break;
                }
                decoded.extend_from_slice(&rest[..size]);
                rest = &rest[size + 2..];
            }
            decoded
        } else {
            body.to_vec()
        };
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn assert_no_fs_path(body: &str, path: &Path) {
        let displayed = path.display().to_string();
        assert!(
            !body.contains(&displayed),
            "response leaked path {displayed}: {body}"
        );
        if let Some(raw) = path.to_str() {
            assert!(!body.contains(raw), "response leaked path {raw}: {body}");
        }
        let slash = displayed.replace('\\', "/");
        if slash != displayed {
            assert!(
                !body.contains(&slash),
                "response leaked path {slash}: {body}"
            );
        }
    }

    fn write_rollout_fixture(rollout_dir: &Path) {
        std::fs::create_dir_all(rollout_dir).unwrap();
        let mut file = std::fs::File::create(rollout_dir.join("session.jsonl")).unwrap();
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-08-24T00:00:00Z",
                "type": "session_meta",
                "payload": {"id": "session-a", "thread_source": "main"}
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-08-24T00:01:00Z",
                "type": "turn_context",
                "payload": {
                    "turn_id": "turn-1",
                    "cwd": "C:/workspace/project",
                    "model": "gpt-test",
                    "effort": "high"
                }
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            json!({
                "timestamp": "2026-08-24T00:01:30Z",
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "last_token_usage": {
                            "input_tokens": 10,
                            "cached_input_tokens": 2,
                            "output_tokens": 5,
                            "reasoning_output_tokens": 1,
                            "total_tokens": 15
                        },
                        "total_token_usage": {
                            "input_tokens": 10,
                            "cached_input_tokens": 2,
                            "output_tokens": 5,
                            "reasoning_output_tokens": 1,
                            "total_tokens": 15
                        },
                        "model_context_window": 1000
                    }
                }
            })
        )
        .unwrap();
    }

    fn sample_event(index: i32) -> CodexTracerEvent {
        CodexTracerEvent::new(
            format!("record-{index}"),
            "session-1".to_string(),
            format!("2026-06-16T10:{:02}:{:02}Z", index / 60, index % 60),
            "/path/to/file1.jsonl".to_string(),
            index,
            1000,
            600,
            200,
            50,
        )
    }

    #[test]
    fn list_query_limit_clamps_to_list_cap() {
        assert_eq!(clamp_list_query_limit(None), LIST_QUERY_LIMIT_MAX);
        assert_eq!(clamp_list_query_limit(Some(1)), 1);
        assert_eq!(clamp_list_query_limit(Some(500)), 500);
        assert_eq!(clamp_list_query_limit(Some(501)), LIST_QUERY_LIMIT_MAX);
        assert_eq!(clamp_list_query_limit(Some(999_999)), LIST_QUERY_LIMIT_MAX);
        assert_eq!(clamp_list_query_limit(Some(0)), LIST_QUERY_LIMIT_MAX);
        assert_eq!(clamp_list_query_limit(Some(-8)), LIST_QUERY_LIMIT_MAX);
    }

    #[tokio::test]
    async fn get_api_refresh_does_not_ingest() {
        let _env_lock = ENV_LOCK.lock().await;
        let dir = tempdir().unwrap();
        let codex_home = dir.path().join("codex-home");
        write_rollout_fixture(&codex_home.join("rollout"));
        let _guard = EnvGuard::set("CODEX_HOME", &codex_home);

        let db_path = dir.path().join("tracer.db");
        let store = CodexTracerStore::open(&db_path).unwrap();
        let addr = serve_test_app(store).await;

        let (status, body) = http_request(addr, "GET", "/api/refresh").await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert!(
            !body.contains("events_found") && !body.contains("\"ok\""),
            "GET /api/refresh must not return an ingest payload: {body}"
        );
        assert_no_fs_path(&body, &codex_home);
        assert_no_fs_path(&body, &codex_home.join("rollout"));

        let (status, body) = http_request(addr, "GET", "/api/stats").await;
        assert_eq!(status, StatusCode::OK);
        let stats: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(stats["total_events"], 0);

        let (status, body) = http_request(addr, "POST", "/api/refresh").await;
        assert_eq!(status, StatusCode::OK);
        let payload: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(payload["ok"], true);
        assert!(payload["events_found"].as_u64().unwrap_or(0) > 0);
        assert!(payload.get("detail").is_none() || payload["detail"].is_null());

        let (status, body) = http_request(addr, "GET", "/api/stats").await;
        assert_eq!(status, StatusCode::OK);
        let stats: Value = serde_json::from_str(&body).unwrap();
        assert!(
            stats["total_events"].as_i64().unwrap_or(0) > 0,
            "POST refresh must ingest so GET ingest would have been visible: {stats}"
        );
    }

    #[tokio::test]
    async fn failure_json_omits_filesystem_paths_and_rusqlite_text() {
        let _env_lock = ENV_LOCK.lock().await;
        let dir = tempdir().unwrap();
        let missing_home = dir.path().join("missing-codex-home");
        let _guard = EnvGuard::set("CODEX_HOME", &missing_home);
        let rollout_dir = missing_home.join("rollout");

        let db_path = dir.path().join("tracer.db");
        let store = CodexTracerStore::open(&db_path).unwrap();
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch("DROP TABLE codex_tracer_events")
                .unwrap();
        }
        let addr = serve_test_app(store).await;

        let (status, body) = http_request(addr, "POST", "/api/refresh").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let payload: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(payload["error"]["code"], "codex_not_found");
        assert_eq!(
            payload["error"]["message"],
            "Codex rollout directory not found"
        );
        assert!(payload["error"]["detail"].is_null());
        assert_no_fs_path(&body, &missing_home);
        assert_no_fs_path(&body, &rollout_dir);

        let (status, body) = http_request(addr, "GET", "/api/calls").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let payload: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(payload["error"]["code"], "internal_error");
        assert_eq!(payload["error"]["message"], "Failed to query calls");
        assert!(payload["error"]["detail"].is_null());
        assert!(!body.to_ascii_lowercase().contains("no such table"));
        assert!(!body.to_ascii_lowercase().contains("rusqlite"));
        assert!(!body.contains("SQLITE"));
        assert_no_fs_path(&body, &db_path);

        let (status, body) = http_request(addr, "GET", "/").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(body.contains("internal_error"));
        assert!(body.contains("Failed to query calls"));
        assert!(!body.to_ascii_lowercase().contains("no such table"));
        assert!(!body.contains(&db_path.display().to_string()));
        assert_no_fs_path(&body, &db_path);
    }

    #[tokio::test]
    async fn list_api_clamps_oversized_limit_below_index_budget() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("tracer.db");
        let mut store = CodexTracerStore::open(&db_path).unwrap();
        let extra = LIST_QUERY_LIMIT_MAX + 1;
        let events: Vec<_> = (0..extra as i32).map(sample_event).collect();
        store.upsert_events(&events).unwrap();
        let addr = serve_test_app(store).await;

        let (status, body) = http_request(addr, "GET", "/api/calls?limit=999999").await;
        assert_eq!(status, StatusCode::OK);
        let payload: Value = serde_json::from_str(&body).unwrap();
        let calls = payload["calls"].as_array().expect("calls array");
        assert_eq!(calls.len(), LIST_QUERY_LIMIT_MAX);
        assert_eq!(payload["count"], LIST_QUERY_LIMIT_MAX);
        let ids: Vec<&str> = calls
            .iter()
            .filter_map(|call| call["record_id"].as_str())
            .collect();
        assert!(!ids.contains(&"record-0"));
        assert!(!body.contains("LIMIT 999999"));

        let (status, body) = http_request(addr, "GET", "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.contains("record-0"),
            "index query budget must exceed the list cap"
        );
        assert!(INDEX_QUERY_LIMIT >= extra);
    }
}
