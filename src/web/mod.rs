use std::{collections::HashMap, net::SocketAddr};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::header,
    response::{Html, IntoResponse},
    routing::get,
};
use serde_json::json;
use tokio::net::TcpListener;

use crate::{query, store::Store};

#[derive(Clone)]
pub struct WebState {
    pub store: Store,
}

pub async fn serve(store: Store, preferred_port: Option<u16>) -> Result<SocketAddr> {
    let state = WebState { store };
    let app = Router::new()
        .route("/", get(index_live))
        .route("/assets/app.css", get(app_css))
        .route("/assets/app.js", get(app_js_live))
        .route("/api/overview", get(api_overview))
        .route("/api/trends", get(api_trends))
        .route("/api/models", get(api_models))
        .route("/api/sources", get(api_sources))
        .route("/api/projects", get(api_projects))
        .route("/api/costs", get(api_costs))
        .route("/api/health", get(api_health))
        .with_state(state);

    let ports = if let Some(port) = preferred_port {
        vec![port]
    } else {
        vec![37421, 37422, 37423, 0]
    };

    for port in ports {
        let listener = TcpListener::bind(("127.0.0.1", port)).await;
        if let Ok(listener) = listener {
            let addr = listener.local_addr()?;
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            return Ok(addr);
        }
    }

    unreachable!("端口探测列表不应为空");
}

pub fn snapshot_index_html() -> String {
    html_shell("snapshot")
}

pub fn live_index_html() -> String {
    html_shell("live")
}

pub fn app_javascript() -> &'static str {
    APP_JS
}

pub fn app_stylesheet() -> &'static str {
    APP_CSS
}

async fn index_live() -> Html<String> {
    Html(live_index_html())
}

async fn app_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        app_stylesheet(),
    )
}

async fn app_js_live() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        app_javascript(),
    )
}

async fn api_overview(
    State(state): State<WebState>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    query::load_overview(&state.store)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_trends(
    State(state): State<WebState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let window = params.get("window").map(String::as_str).unwrap_or("day");
    query::load_trends(&state.store, window)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_models(
    State(state): State<WebState>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    query::load_model_breakdown(&state.store)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_sources(
    State(state): State<WebState>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    query::load_source_breakdown(&state.store)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_projects(
    State(state): State<WebState>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    query::load_project_breakdown(&state.store)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_costs(
    State(state): State<WebState>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    query::load_cost_breakdown(&state.store)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_health(
    State(state): State<WebState>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    query::load_health(&state.store)
        .map(|value| Json(json!(value)))
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

fn html_shell(mode: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>llmusage 本地分析页</title>
    <link rel="stylesheet" href="assets/app.css" />
  </head>
  <body data-mode="{mode}">
    <main class="shell">
      <header class="hero">
        <div>
          <p class="eyebrow">LOCAL ONLY</p>
          <h1>llmusage 本地分析页</h1>
          <p class="subhead">Codex、Claude、OpenCode 的本地用量、趋势、项目、成本与健康状态。</p>
        </div>
        <div id="meta" class="meta"></div>
      </header>
      <section id="overview" class="cards"></section>
      <section class="panel">
        <div class="panel-header">
          <h2>趋势</h2>
          <div class="window-switch">
            <button data-window="day">24h</button>
            <button data-window="week">7d</button>
            <button data-window="month">30d</button>
            <button data-window="all">全部</button>
          </div>
        </div>
        <div id="trends" class="chart-list"></div>
      </section>
      <div class="grid">
        <section class="panel"><h2>模型</h2><div id="models"></div></section>
        <section class="panel"><h2>来源</h2><div id="sources"></div></section>
        <section class="panel"><h2>项目</h2><div id="projects"></div></section>
        <section class="panel"><h2>成本</h2><div id="costs"></div></section>
      </div>
      <section class="panel">
        <h2>健康</h2>
        <div id="health"></div>
      </section>
    </main>
    <script src="assets/app.js"></script>
  </body>
</html>"#
    )
}

const APP_CSS: &str = r#"
:root {
  --bg: #07111f;
  --panel: rgba(16, 25, 43, 0.9);
  --line: rgba(120, 146, 199, 0.28);
  --text: #e8eefc;
  --muted: #94a8d0;
  --accent: #57d0ff;
  --accent-soft: rgba(87, 208, 255, 0.16);
  --success: #67e8a5;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: "Segoe UI", "PingFang SC", sans-serif;
  color: var(--text);
  background:
    radial-gradient(circle at top right, rgba(87, 208, 255, 0.2), transparent 30%),
    linear-gradient(180deg, #07111f 0%, #02060e 100%);
}
.shell {
  max-width: 1180px;
  margin: 0 auto;
  padding: 32px 20px 48px;
}
.hero {
  display: flex;
  justify-content: space-between;
  gap: 20px;
  align-items: flex-end;
  margin-bottom: 24px;
}
.eyebrow {
  margin: 0 0 10px;
  color: var(--accent);
  letter-spacing: 0.2em;
  font-size: 12px;
}
h1, h2 { margin: 0; }
.subhead {
  margin: 10px 0 0;
  color: var(--muted);
  max-width: 720px;
}
.meta {
  padding: 14px 16px;
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 16px;
  min-width: 220px;
}
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 14px;
  margin-bottom: 22px;
}
.card, .panel {
  border: 1px solid var(--line);
  background: var(--panel);
  border-radius: 18px;
  padding: 18px;
}
.metric {
  font-size: 30px;
  font-weight: 700;
  margin-top: 6px;
}
.hint { color: var(--muted); font-size: 13px; }
.panel { margin-bottom: 18px; }
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}
.window-switch { display: flex; gap: 8px; flex-wrap: wrap; }
.window-switch button {
  border: 1px solid var(--line);
  background: transparent;
  color: var(--text);
  border-radius: 999px;
  padding: 8px 12px;
  cursor: pointer;
}
.window-switch button.active {
  background: var(--accent-soft);
  border-color: var(--accent);
}
.chart-list {
  display: grid;
  gap: 8px;
}
.trend-row, .table-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 12px;
  align-items: center;
  padding: 10px 0;
  border-bottom: 1px solid rgba(255,255,255,0.05);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 18px;
}
.bar {
  height: 8px;
  border-radius: 999px;
  background: rgba(255,255,255,0.08);
  overflow: hidden;
  margin-top: 6px;
}
.bar > span {
  display: block;
  height: 100%;
  background: linear-gradient(90deg, var(--accent), #7cf3d4);
}
.mono { font-family: Consolas, "SFMono-Regular", monospace; }
.ok { color: var(--success); }
pre {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--muted);
}
@media (max-width: 760px) {
  .hero { flex-direction: column; align-items: stretch; }
}
"#;

const APP_JS: &str = r#"
const mode = document.body.dataset.mode || 'live';
const state = { window: 'day', snapshot: null };

async function loadJson(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Request failed: ${response.status}`);
  }
  return response.json();
}

async function loadSection(section, path) {
  if (mode === 'snapshot') {
    if (!state.snapshot) {
      state.snapshot = await loadJson('snapshot.json');
    }
    return state.snapshot[section];
  }
  return loadJson(path);
}

function formatNumber(value) {
  return Intl.NumberFormat('en-US').format(value || 0);
}

function formatUsd(value) {
  return `$${(value || 0).toFixed(2)}`;
}

function renderOverview(overview) {
  document.getElementById('meta').innerHTML = `
    <div class="hint">生成时间</div>
    <div>${overview.generated_at}</div>
    <div class="hint" style="margin-top:10px;">最近同步</div>
    <div>${overview.last_sync_at || 'never'}</div>
  `;
  const cards = [
    ['总 tokens', overview.total.total_tokens, '累计用量'],
    ['24h tokens', overview.last_24h.total_tokens, '最近 24 小时'],
    ['来源数', overview.source_count, '活跃数据源'],
    ['bucket 数', overview.bucket_count, '30 分钟桶']
  ];
  document.getElementById('overview').innerHTML = cards.map(([label, value, hint]) => `
    <article class="card">
      <div class="hint">${label}</div>
      <div class="metric">${formatNumber(value)}</div>
      <div class="hint">${hint}</div>
    </article>
  `).join('');
}

function renderTrendRows(points) {
  const max = Math.max(1, ...points.map(point => point.total_tokens || 0));
  document.getElementById('trends').innerHTML = points.map(point => `
    <div class="trend-row">
      <div>
        <div>${point.label}</div>
        <div class="bar"><span style="width:${(point.total_tokens / max) * 100}%"></span></div>
      </div>
      <div class="mono">${formatNumber(point.total_tokens)}</div>
    </div>
  `).join('') || '<div class="hint">暂无趋势数据</div>';
}

function renderKeyValueList(targetId, rows, formatter) {
  document.getElementById(targetId).innerHTML = rows.map(row => formatter(row)).join('') || '<div class="hint">暂无数据</div>';
}

async function refresh() {
  const overview = await loadSection('overview', '/api/overview');
  renderOverview(overview);

  const trends = mode === 'snapshot'
    ? state.snapshot[`${state.window}_trends`]
    : await loadJson(`/api/trends?window=${state.window}`);
  renderTrendRows(trends);

  const models = await loadSection('models', '/api/models');
  renderKeyValueList('models', models, row => `
    <div class="table-row">
      <div>${row.model}</div>
      <div class="mono">${formatNumber(row.total_tokens)}</div>
    </div>
  `);

  const sources = await loadSection('sources', '/api/sources');
  renderKeyValueList('sources', sources, row => `
    <div class="table-row">
      <div>
        <div>${row.source}</div>
        <div class="hint">${row.last_event_at || 'never'}</div>
      </div>
      <div class="mono">${formatNumber(row.total_tokens)}</div>
    </div>
  `);

  const projects = await loadSection('projects', '/api/projects');
  renderKeyValueList('projects', projects, row => `
    <div class="table-row">
      <div>
        <div>${row.project_label}</div>
        <div class="hint">${row.project_ref || row.project_hash}</div>
      </div>
      <div class="mono">${formatNumber(row.total_tokens)}</div>
    </div>
  `);

  const costs = await loadSection('costs', '/api/costs');
  renderKeyValueList('costs', costs, row => `
    <div class="table-row">
      <div>
        <div>${row.model}</div>
        <div class="hint">${row.source}</div>
      </div>
      <div class="mono">${formatUsd(row.estimated_cost_usd)}</div>
    </div>
  `);

  const health = await loadSection('health', '/api/health');
  document.getElementById('health').innerHTML = `
    <div class="grid">
      <div>
        <div class="hint">集成状态</div>
        ${(health.integrations || []).map(item => `<div class="table-row"><div>${item.source}</div><div class="mono">${item.status}</div></div>`).join('') || '<div class="hint">暂无记录</div>'}
      </div>
      <div>
        <div class="hint">Cursor</div>
        ${(health.cursors || []).map(item => `<div class="table-row"><div>${item.source}:${item.cursor_key}</div><div class="mono">${item.sqlite_status || item.updated_at || 'ok'}</div></div>`).join('') || '<div class="hint">暂无 cursor</div>'}
      </div>
      <div>
        <div class="hint">最近失败</div>
        <pre>${JSON.stringify(health.recent_failures || [], null, 2)}</pre>
      </div>
    </div>
  `;
}

document.querySelectorAll('[data-window]').forEach(button => {
  button.addEventListener('click', async () => {
    state.window = button.dataset.window;
    document.querySelectorAll('[data-window]').forEach(item => item.classList.remove('active'));
    button.classList.add('active');
    await refresh();
  });
});

document.querySelector('[data-window="day"]').classList.add('active');
refresh().catch(error => {
  document.body.innerHTML = `<main class="shell"><section class="panel"><h2>加载失败</h2><pre>${error.stack || error.message}</pre></section></main>`;
});
"#;
