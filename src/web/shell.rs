/*
 * ========================================================================
 * 步骤1：生成 live / snapshot 共用页面骨架
 * ========================================================================
 * 目标：
 * 1) 左侧 248px 固定侧边栏 + 右侧主区
 * 2) 让 live / snapshot 只通过 data-mode 区分数据来源
 * 3) 改为浏览器原生 ES module 加载入口脚本
 */
pub fn live_index_html() -> String {
    html_shell("live")
}

pub fn snapshot_index_html() -> String {
    html_shell("snapshot")
}

fn html_shell(mode: &str) -> String {
    let environment_chip = if mode == "snapshot" {
        "离线文件"
    } else {
        "仅本地"
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>llmusage · 本地用量概览</title>
<link rel="stylesheet" href="assets/base.css" />
<link rel="stylesheet" href="assets/layout.css" />
<link rel="stylesheet" href="assets/components.css" />
<link rel="stylesheet" href="assets/charts.css" />
</head>
<body data-mode="{mode}">

<div class="app">
  <!-- Sidebar -->
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">l</div>
      <div>
        <div class="brand-name">llmusage</div>
        <div class="brand-sub">v0.4.2 · local</div>
      </div>
    </div>

    <div class="nav-label">概览</div>
    <nav>
      <a href="#overview" class="active" data-target="overview">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg></span>
        <span>用量概览</span>
        <span class="badge">4</span>
      </a>
      <a href="#trends" data-target="trends">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><polyline points="3,17 9,11 13,15 21,7"/><polyline points="14,7 21,7 21,14"/></svg></span>
        <span>用量趋势</span>
        <span class="badge">24h</span>
      </a>
    </nav>

    <div class="nav-label">分布</div>
    <nav>
      <a href="#models" data-target="models">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/></svg></span>
        <span>模型分布</span>
      </a>
      <a href="#sources" data-target="sources">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M12 3l9 5v8l-9 5-9-5V8z"/><path d="M3 8l9 5 9-5M12 13v9"/></svg></span>
        <span>来源分布</span>
      </a>
      <a href="#projects" data-target="projects">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M3 7h6l2 2h10v11H3z"/></svg></span>
        <span>项目排行</span>
      </a>
    </nav>

    <div class="nav-label">运营</div>
    <nav>
      <a href="#cost" data-target="cost">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M15 9h-4a2 2 0 100 4h2a2 2 0 110 4H9M12 7v2M12 15v2"/></svg></span>
        <span>成本估算</span>
      </a>
      <a href="#status" data-target="status">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg></span>
        <span>运行状态</span>
      </a>
    </nav>

    <div class="sidebar-footer">
      <div class="endpoint">
        <div class="endpoint-row">
          <span class="pulse"></span>
          <span class="endpoint-host" id="endpoint-host">127.0.0.1:37421</span>
        </div>
        <div class="endpoint-meta">
          <span>最近同步</span>
          <span class="mono" id="endpoint-sync">--</span>
        </div>
      </div>
    </div>
  </aside>

  <!-- Main content -->
  <main>
    <!-- Topbar -->
    <div class="topbar">
      <div class="crumbs">
        <span>llmusage</span>
        <span class="sep">/</span>
        <span>dashboard</span>
        <span class="sep">/</span>
        <strong>本地用量概览</strong>
      </div>
      <div class="topbar-actions">
        <span class="tag local">
          <svg class="i" viewBox="0 0 24 24" style="width: 11px; height: 11px;"><path d="M12 22s-8-4.5-8-11a8 8 0 1116 0c0 6.5-8 11-8 11z"/><circle cx="12" cy="11" r="3"/></svg>
          {environment_chip}
        </span>
        <button class="btn" id="btn-export">
          <svg class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"/></svg>
          导出 JSON
        </button>
        <button class="btn btn-primary" id="btn-sync">
          <svg class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15A9 9 0 1 1 18 6.36L23 10"/></svg>
          同步
        </button>
      </div>
    </div>

    <!-- Hero + Status -->
    <section id="overview" class="block">
      <div class="hero">
        <div>
          <div class="section-eyebrow" style="margin-bottom: 10px;">DASHBOARD</div>
          <h1 class="hero-title">本地用量<span class="accent">概览</span></h1>
          <p class="hero-desc">
            本地查看近期用量、成本估算和运行状态。所有数据存放在本机 SQLite 中，不依赖任何外部接口、不上报任何遥测，可放心断网使用。
          </p>
          <div class="hero-meta" id="hero-meta"></div>
        </div>

        <!-- Run summary card -->
        <div class="status-panel" id="status-panel"></div>
      </div>

      <!-- KPI cards -->
      <div class="kpi-grid" id="kpi-grid"></div>
    </section>

    <!-- Trends -->
    <section id="trends" class="block">
      <div class="trends-card">
        <div class="trends-head">
          <div class="trends-title-block">
            <div class="section-eyebrow">TRENDS</div>
            <div class="trends-title">用量趋势</div>
            <div class="trends-sub">主图展示当前窗口内最近 10 条记录，完整明细可展开查看。</div>
          </div>
          <div class="seg" id="seg">
            <button class="active" data-window="day">24h</button>
            <button data-window="week">7d</button>
            <button data-window="month">30d</button>
            <button data-window="all">全部</button>
          </div>
        </div>

        <div class="trends-stats" id="trends-stats"></div>

        <div class="trends-chart-wrap">
          <div class="trends-chart-head">
            <div class="chart-title">最近 10 个时段</div>
            <div class="chart-legend">
              <span><span class="legend-dot"></span>用量 (Token)</span>
            </div>
          </div>
          <svg class="chart-svg" viewBox="0 0 720 220" preserveAspectRatio="none" id="trends-chart">
            <g stroke="rgba(255,255,255,0.06)" stroke-dasharray="2 4">
              <line x1="0" y1="40" x2="720" y2="40"/>
              <line x1="0" y1="80" x2="720" y2="80"/>
              <line x1="0" y1="120" x2="720" y2="120"/>
              <line x1="0" y1="160" x2="720" y2="160"/>
            </g>
            <g id="trends-bars" fill="#c8553d"></g>
            <line x1="0" y1="200" x2="720" y2="200" stroke="rgba(255,255,255,0.15)"/>
            <g id="trends-labels" fill="#8d867a" font-family="JetBrains Mono, monospace" font-size="9.5"></g>
          </svg>
        </div>

        <div class="trends-bottom">
          <div id="trends-table"></div>
          <div id="trends-sources"></div>
        </div>
      </div>
    </section>

    <!-- Models + Sources/Projects side by side -->
    <section id="models" class="block">
      <div class="section-head">
        <div>
          <div class="section-eyebrow">MODELS</div>
          <h2 class="section-title">模型用量分布</h2>
          <div class="section-desc">先看用量最高的模型，再按需展开完整排行。</div>
        </div>
      </div>

      <div class="grid-2">
        <div class="panel">
          <div class="panel-title">用量最高的 8 个模型</div>
          <div class="panel-sub">单位：Token，按累计计算</div>

          <div id="models-bars"></div>
          <div id="models-table"></div>

          <button class="show-more" data-toggle-panel="models">展开完整排行 →</button>
        </div>

        <div>
          <div class="panel" id="sources" style="margin-bottom: 24px;">
            <div class="section-eyebrow">SOURCES</div>
            <div style="display: flex; justify-content: space-between; align-items: baseline;">
              <h3 style="font-size: 18px; font-weight: 600; letter-spacing: -0.018em; margin: 4px 0 4px;">来源分布</h3>
              <span class="tag" id="sources-count">-- 个来源</span>
            </div>
            <div class="panel-sub">用量最高的 4 个来源</div>

            <div class="source-rows" id="sources-rows"></div>
          </div>

          <div class="panel" id="projects">
            <div class="section-eyebrow">PROJECTS</div>
            <div style="display: flex; justify-content: space-between; align-items: baseline;">
              <h3 style="font-size: 18px; font-weight: 600; letter-spacing: -0.018em; margin: 4px 0 4px;">项目排行</h3>
              <span class="tag" id="projects-count">-- 个项目</span>
            </div>
            <div class="panel-sub">按累计 Token 排序</div>

            <div class="project-list" id="projects-rows"></div>

            <button class="show-more" data-toggle-panel="projects">展开全部项目 →</button>
          </div>
        </div>
      </div>
    </section>

    <!-- Cost -->
    <section id="cost" class="block">
      <div class="section-head">
        <div>
          <div class="section-eyebrow">COST</div>
          <h2 class="section-title">成本估算</h2>
          <div class="section-desc">基于公开计价表的本地估算。仅供参考，与账单存在差异。</div>
        </div>
      </div>

      <div class="grid-2">
        <div class="panel">
          <div class="panel-title">成本最高的 5 个 来源 / 模型 组合</div>
          <div class="panel-sub">单位：USD</div>

          <div id="costs-bars" style="margin-top: 18px;"></div>
          <div id="costs-table"></div>

          <button class="show-more" data-toggle-panel="costs">展开全部成本项 →</button>
        </div>

        <div id="status" class="block" style="margin: 0;">
          <div class="panel">
            <div class="cost-grid" id="costs-stats"></div>

            <div style="border-top: 1px dashed var(--line); padding-top: 18px;">
              <div class="section-eyebrow">FAILURES</div>
              <h3 style="font-size: 16px; font-weight: 600; margin: 4px 0 12px;">最近失败</h3>
              <div id="failures-card"></div>
            </div>

            <div style="border-top: 1px dashed var(--line); padding-top: 18px; margin-top: 18px;">
              <div class="section-eyebrow">INTEGRATIONS</div>
              <h3 style="font-size: 16px; font-weight: 600; margin: 4px 0 12px;">集成状态</h3>
              <div id="integrations-rows" style="display: grid; gap: 10px;"></div>
            </div>
          </div>
        </div>
      </div>
    </section>

    <footer class="foot">
      <span>llmusage v0.4.2 · build 2026.05.06</span>
      <span><a href="#overview">回到顶部 ↑</a></span>
    </footer>
  </main>
</div>

<script type="module" src="assets/app.js"></script>
</body>
</html>"##,
        mode = mode,
        environment_chip = environment_chip
    )
}
