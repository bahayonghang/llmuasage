/*
 * ========================================================================
 * 步骤1：生成 live / snapshot 共用页面骨架
 * ========================================================================
 * 目标：
 * 1) 左侧 248px 固定侧边栏 + 右侧主区
 * 2) 让 live / snapshot 只通过 data-mode 区分数据来源
 * 3) 改为浏览器原生 ES module 加载入口脚本
 * 4) 全部中文短语挂 data-i18n key，运行时由 JS 端按 locale 替换
 * 5) <html> 上提前写入 data-theme / data-locale，避免主题切换闪烁
 */
pub fn live_index_html() -> String {
    html_shell("live")
}

pub fn snapshot_index_html() -> String {
    html_shell("snapshot")
}

fn html_shell(mode: &str) -> String {
    let (environment_chip, environment_chip_key) = if mode == "snapshot" {
        ("离线文件", "shell.tag.snapshot")
    } else {
        ("仅本地", "shell.tag.local")
    };
    let app_version = env!("CARGO_PKG_VERSION");
    let supported_sources = crate::registry::registered_source_descriptors()
        .iter()
        .map(|descriptor| descriptor.stable_id)
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r##"<!DOCTYPE html>
<html lang="zh-CN" data-theme="light" data-locale="zh" data-i18n-title="shell.window.title">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>llmusage · 本地用量概览</title>
<script>
(function(){{
  try {{
    var t = localStorage.getItem('llmusage:theme');
    var l = localStorage.getItem('llmusage:locale');
    if (t !== 'dark' && t !== 'light') {{
      t = (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) ? 'dark' : 'light';
    }}
    document.documentElement.setAttribute('data-theme', t);
    if (l === 'zh' || l === 'en') {{
      document.documentElement.setAttribute('data-locale', l);
    }}
  }} catch (_) {{}}
}})();
</script>
<link rel="icon" type="image/svg+xml" href="assets/favicon.svg" />
<link rel="stylesheet" href="assets/base.css" />
<link rel="stylesheet" href="assets/layout.css" />
<link rel="stylesheet" href="assets/components.css" />
<link rel="stylesheet" href="assets/charts.css" />
</head>
<body data-mode="{mode}" data-app-version="{app_version}" data-supported-sources="{supported_sources}">

<div class="app">
  <!-- Sidebar -->
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">{brand_mark}</div>
      <div>
        <div class="brand-name">llmusage</div>
        <div class="brand-sub">v{app_version} · local</div>
      </div>
    </div>

    <div class="nav-label" id="nav-label-overview" data-i18n="shell.nav.label.overview">概览</div>
    <nav aria-labelledby="nav-label-overview">
      <a href="#overview" class="active" data-target="overview">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg></span>
        <span data-i18n="shell.nav.item.usage">用量概览</span>
      </a>
      <a href="#trends" data-target="trends">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><polyline points="3,17 9,11 13,15 21,7"/><polyline points="14,7 21,7 21,14"/></svg></span>
        <span data-i18n="shell.nav.item.trend">用量趋势</span>
      </a>
    </nav>

    <div class="nav-label" id="nav-label-distribution" data-i18n="shell.nav.label.distribution">分布</div>
    <nav aria-labelledby="nav-label-distribution">
      <a href="#models" data-target="models">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/></svg></span>
        <span data-i18n="shell.nav.item.models">模型分布</span>
      </a>
      <a href="#sources" data-target="sources">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><path d="M12 3l9 5v8l-9 5-9-5V8z"/><path d="M3 8l9 5 9-5M12 13v9"/></svg></span>
        <span data-i18n="shell.nav.item.sources">来源分布</span>
      </a>
      <a href="#projects" data-target="projects">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><path d="M3 7h6l2 2h10v11H3z"/></svg></span>
        <span data-i18n="shell.nav.item.projects">项目排行</span>
      </a>
      <a href="#behavior" data-target="behavior">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><path d="M4 19V5"/><path d="M4 19h16"/><path d="M8 16v-5"/><path d="M12 16V8"/><path d="M16 16v-7"/></svg></span>
        <span data-i18n="shell.nav.item.behavior">行为分析</span>
      </a>
      <a href="#explorer" data-target="explorer">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><circle cx="11" cy="11" r="7"/><path d="M21 21l-5-5"/><path d="M8 11h6M11 8v6"/></svg></span>
        <span data-i18n="shell.nav.item.explorer">切片分析</span>
      </a>
    </nav>

    <div class="nav-label" id="nav-label-ops" data-i18n="shell.nav.label.ops">运营</div>
    <nav aria-labelledby="nav-label-ops">
      <a href="#cost" data-target="cost">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M15 9h-4a2 2 0 100 4h2a2 2 0 110 4H9M12 7v2M12 15v2"/></svg></span>
        <span data-i18n="shell.nav.item.cost">成本估算</span>
      </a>
      <a href="#status" data-target="status">
        <span class="nav-icon"><svg aria-hidden="true" class="i" viewBox="0 0 24 24"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg></span>
        <span data-i18n="shell.nav.item.status">运行状态</span>
      </a>
    </nav>

    <div class="sidebar-footer">
      <div class="sidebar-toggles" role="group" data-i18n-attr="aria-label=toolbar.group.aria">
        <button class="toggle-btn" id="toggle-theme" type="button" data-i18n-attr="aria-label=toolbar.theme.aria">
          <svg aria-hidden="true" class="i toggle-icon icon-sun" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M5 19l1.5-1.5M17.5 6.5L19 5"/></svg>
          <svg aria-hidden="true" class="i toggle-icon icon-moon" viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
          <span class="toggle-label label-to-dark" data-i18n="toolbar.theme.toDark">深色</span>
          <span class="toggle-label label-to-light" data-i18n="toolbar.theme.toLight">浅色</span>
        </button>
        <button class="toggle-btn" id="toggle-locale" type="button" data-i18n-attr="aria-label=toolbar.lang.aria">
          <span class="toggle-glyph glyph-zh" data-i18n="toolbar.lang.label.zh">中</span>
          <span class="toggle-glyph glyph-en">A</span>
          <span class="toggle-label label-zh">ZH</span>
          <span class="toggle-label label-en">EN</span>
        </button>
      </div>
      <div class="endpoint">
        <div class="endpoint-row">
          <span class="pulse"></span>
          <span class="endpoint-host" id="endpoint-host">127.0.0.1:37421</span>
        </div>
        <div class="endpoint-meta">
          <span data-i18n="shell.endpoint.lastSync">最近同步</span>
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
        <span data-i18n="shell.crumb.dashboard">dashboard</span>
        <span class="sep">/</span>
        <strong data-i18n="shell.crumb.local">本地用量概览</strong>
      </div>
      <div class="topbar-actions">
        <span class="tag local">
          <svg aria-hidden="true" class="i" viewBox="0 0 24 24" style="width: 11px; height: 11px;"><path d="M12 22s-8-4.5-8-11a8 8 0 1116 0c0 6.5-8 11-8 11z"/><circle cx="12" cy="11" r="3"/></svg>
          <span data-i18n="{environment_chip_key}">{environment_chip}</span>
        </span>
        <button class="btn" id="btn-export">
          <svg aria-hidden="true" class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"/></svg>
          <span data-i18n="shell.btn.export">导出 JSON</span>
        </button>
        <div class="refresh-toggle" id="auto-refresh" role="group" data-i18n-attr="aria-label=shell.refresh.aria">
          <span class="refresh-label" data-i18n="shell.refresh.label">刷新</span>
          <button type="button" data-refresh-interval="0" aria-pressed="true" data-i18n="shell.refresh.off">关闭</button>
          <button type="button" data-refresh-interval="30000" aria-pressed="false">30s</button>
          <button type="button" data-refresh-interval="60000" aria-pressed="false">60s</button>
        </div>
        <button class="btn btn-primary" id="btn-sync">
          <svg aria-hidden="true" class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15A9 9 0 1 1 18 6.36L23 10"/></svg>
          <span data-i18n="shell.btn.sync">同步</span>
        </button>
      </div>
    </div>

    <!-- Hero + Status -->
    <section id="overview" class="block">
      <div class="hero">
        <div>
          <h1 class="hero-title" data-i18n-html="shell.hero.title.html">本地用量<span class="accent">概览</span></h1>
          <p class="hero-desc" data-i18n="shell.hero.desc">
            本地查看近期用量、成本估算和运行状态。所有数据存放在本机 SQLite 中，不依赖任何外部接口、不上报任何遥测，可放心断网使用。
          </p>
          <div class="hero-meta" id="hero-meta"></div>
        </div>

        <!-- Run summary card -->
        <div class="status-panel" id="status-panel"></div>
      </div>

      <div class="filter-rail" id="filter-rail" aria-label="Dashboard filters">
        <div class="filter-group">
          <label for="filter-source" data-i18n="shell.filters.source">来源</label>
          <select id="filter-source" data-filter="source">
            <option value="all" data-i18n="shell.filters.allSources">全部来源</option>
          </select>
        </div>
        <div class="filter-group">
          <label for="filter-model" data-i18n="shell.filters.model">模型</label>
          <input id="filter-model" data-filter="model" type="search" placeholder="all models" data-i18n-attr="placeholder=shell.filters.modelPlaceholder" />
        </div>
        <div class="filter-group filter-range-group">
          <label id="range-presets-label" data-i18n="shell.filters.range">时间范围</label>
          <div class="range-presets" id="range-presets" role="group" aria-labelledby="range-presets-label" data-i18n-attr="aria-label=shell.filters.rangeAria">
            <button type="button" data-range-preset="1d" aria-pressed="true" data-i18n="shell.filters.range.1d">近 1 天</button>
            <button type="button" data-range-preset="7d" aria-pressed="false" data-i18n="shell.filters.range.7d">近 7 天</button>
            <button type="button" data-range-preset="30d" aria-pressed="false" data-i18n="shell.filters.range.30d">近 30 天</button>
            <button type="button" data-range-preset="all" aria-pressed="false" data-i18n="shell.filters.range.all">全部</button>
          </div>
        </div>
        <div class="filter-group">
          <label for="filter-since" data-i18n="shell.filters.since">起始日期</label>
          <input id="filter-since" data-filter="since" data-date-input type="text" inputmode="numeric" autocomplete="off" placeholder="YYYY-MM-DD" data-i18n-attr="placeholder=shell.filters.datePlaceholder" />
        </div>
        <div class="filter-group">
          <label for="filter-until" data-i18n="shell.filters.until">结束日期</label>
          <input id="filter-until" data-filter="until" data-date-input type="text" inputmode="numeric" autocomplete="off" placeholder="YYYY-MM-DD" data-i18n-attr="placeholder=shell.filters.datePlaceholder" />
        </div>
        <div class="filter-actions">
          <button class="btn btn-primary" id="filters-apply" type="button" data-i18n="shell.filters.apply">应用筛选</button>
          <button class="btn" id="filters-reset" type="button" data-i18n="shell.filters.reset">重置</button>
        </div>
      </div>

      <!-- KPI cards -->
      <div class="kpi-grid" id="kpi-grid"></div>

      <div class="sync-command-center" id="sync-command-center" aria-live="polite">
        <div class="sync-command-center-empty">
          <div class="section-eyebrow" data-i18n="shell.syncCenter.eyebrow">SYNC</div>
          <div data-i18n="shell.syncCenter.loading">正在读取同步状态…</div>
        </div>
      </div>
    </section>

    <!-- Trends -->
    <section id="trends" class="block">
      <div class="trends-card">
        <div class="trends-head">
          <div class="trends-title-block">
            <h2 class="trends-title" data-i18n="shell.trends.title">用量趋势</h2>
            <div class="trends-sub" data-i18n="shell.trends.sub">主图展示当前窗口内最近 10 条记录，完整明细可展开查看。</div>
          </div>
          <div class="seg" id="seg" role="group" data-i18n-attr="aria-label=shell.trends.windowAria">
            <button type="button" class="active" data-window="day" aria-pressed="true">24h</button>
            <button type="button" data-window="week" aria-pressed="false">7d</button>
            <button type="button" data-window="month" aria-pressed="false">30d</button>
            <button type="button" data-window="all" aria-pressed="false" data-i18n="seg.all">全部</button>
          </div>
        </div>

        <div class="trends-stats" id="trends-stats"></div>

        <div class="trends-chart-wrap">
          <div class="trends-chart-head">
            <div class="chart-title" data-i18n="shell.trends.chart.recent10">最近 10 个时段</div>
            <div class="chart-legend">
              <span><span class="legend-dot"></span><span data-i18n="shell.trends.legend.tokens">用量 (Token)</span></span>
            </div>
          </div>
          <svg class="chart-svg trends-chart-svg" viewBox="0 0 720 220" id="trends-chart" role="img" aria-label="最近 10 个时段用量趋势">
            <g class="trend-grid-lines">
              <line x1="0" y1="40" x2="100%" y2="40"/>
              <line x1="0" y1="80" x2="100%" y2="80"/>
              <line x1="0" y1="120" x2="100%" y2="120"/>
              <line x1="0" y1="160" x2="100%" y2="160"/>
            </g>
            <g id="trends-bars" fill="#c8553d"></g>
            <line class="trend-baseline" x1="0" y1="200" x2="100%" y2="200"/>
            <g id="trends-labels" fill="#8d867a" font-size="10.5"></g>
          </svg>
        </div>

        <div class="trends-bottom">
          <div id="trends-table"></div>
          <div id="trends-sources"></div>
        </div>
      </div>
    </section>

    <!-- Models + Sources/Projects distribution workbench -->
    <section id="models" class="block">
      <div class="section-head">
        <div>
          <h2 class="section-title" data-i18n="shell.models.title">模型用量分布</h2>
          <div class="section-desc" data-i18n="shell.models.sub">先看用量最高的模型，再按需展开完整排行。</div>
        </div>
      </div>

      <div class="distribution-grid">
        <div class="panel distribution-models">
          <div class="panel-title" data-i18n="shell.models.panelTitle">用量最高的 8 个模型</div>
          <div class="panel-sub" data-i18n="shell.models.panelSub">单位：Token，按累计计算</div>

          <div class="panel-bars" id="models-bars"></div>
          <div id="models-table"></div>

          <button class="show-more" type="button" data-toggle-panel="models" aria-expanded="false" data-i18n="shell.models.expand">展开完整排行 →</button>
        </div>

        <div class="panel distribution-sources" id="sources">
          <div class="panel-head">
            <h3 class="panel-title" data-i18n="shell.sources.title">来源分布</h3>
            <span class="tag" id="sources-count">--</span>
          </div>
          <div class="panel-sub" data-i18n="shell.sources.sub">用量最高的 4 个来源</div>

          <div class="source-rows" id="sources-rows"></div>
        </div>

        <div class="panel distribution-projects" id="projects">
          <div class="panel-head">
            <h3 class="panel-title" data-i18n="shell.projects.title">项目排行</h3>
            <span class="tag" id="projects-count">--</span>
          </div>
          <div class="panel-sub" data-i18n="shell.projects.sub">按累计 Token 排序</div>

          <div class="project-list" id="projects-rows"></div>

          <button class="show-more" type="button" data-toggle-panel="projects" aria-expanded="false" data-i18n="shell.projects.expand">展开全部项目 →</button>
        </div>
      </div>
    </section>

    <!-- Behavior -->
    <section id="behavior" class="block">
      <div class="section-head">
        <div>
          <h2 class="section-title" data-i18n="shell.behavior.title">行为分析</h2>
          <div class="section-desc" data-i18n="shell.behavior.sub">基于同步阶段提取的 normalized turn/tool facts；低样本或未支持来源会显式显示降级状态。</div>
        </div>
      </div>

      <div class="behavior-grid">
        <div class="panel behavior-primary">
          <div class="panel-head">
            <div>
              <div class="panel-title" data-i18n="shell.behavior.activity.title">Activity</div>
              <div class="panel-sub" data-i18n="shell.behavior.activity.sub">按 turn category 聚合 turns、one-shot 与 retry</div>
            </div>
            <span class="tag" id="activity-support">--</span>
          </div>
          <div class="panel-bars" id="activity-bars"></div>
          <div id="activity-table"></div>
        </div>

        <div class="panel behavior-primary">
          <div class="panel-head">
            <div>
              <div class="panel-title" data-i18n="shell.behavior.tools.title">Tools</div>
              <div class="panel-sub" data-i18n="shell.behavior.tools.sub">Core tools / shell / MCP / agent actions</div>
            </div>
            <span class="tag" id="tools-support">--</span>
          </div>
          <div class="panel-bars" id="tools-bars"></div>
          <div id="tools-table"></div>
        </div>

        <div class="panel behavior-secondary">
          <div>
            <div class="panel-title" data-i18n="shell.behavior.optimize.title">Optimize</div>
            <div class="panel-sub" data-i18n="shell.behavior.optimize.sub">只读浪费检测；不会自动执行删除、归档或重写。</div>
          </div>
          <div id="optimize-summary" class="mini-stat-grid"></div>
          <div id="optimize-findings" class="finding-list"></div>
        </div>

        <div class="panel behavior-secondary">
          <div>
            <div class="panel-title" data-i18n="shell.behavior.compare.title">Compare</div>
            <div class="panel-sub" data-i18n="shell.behavior.compare.sub">按模型对比成本、one-shot、retry 与工作风格；低样本显式提示。</div>
          </div>
          <div id="compare-panel"></div>
        </div>
      </div>
    </section>

    <!-- Explorer -->
    <section id="explorer" class="block">
      <div class="section-head">
        <div>
          <h2 class="section-title" data-i18n="shell.explorer.title">Cost Explorer</h2>
          <div class="section-desc" data-i18n="shell.explorer.sub">按时间粒度、指标、维度与工具过滤做本地切片分析；结果来自后端聚合，不在前端透视原始行。</div>
        </div>
        <span class="tag" id="explorer-support">--</span>
      </div>

      <div class="panel explorer-workbench">
        <div class="explorer-controls" id="explorer-controls">
          <div class="filter-group">
            <label for="explorer-metric" data-i18n="shell.explorer.metric">指标</label>
            <select id="explorer-metric" data-explorer-control="metric">
              <option value="attributed_cost_usd" data-i18n="shell.explorer.metric.cost">归因成本</option>
              <option value="calls" data-i18n="shell.explorer.metric.calls">调用数</option>
              <option value="turns" data-i18n="shell.explorer.metric.turns">Turns</option>
              <option value="sessions" data-i18n="shell.explorer.metric.sessions">会话数</option>
              <option value="total_tokens" data-i18n="shell.explorer.metric.tokens">总 Token</option>
            </select>
          </div>
          <div class="filter-group">
            <label for="explorer-group-by" data-i18n="shell.explorer.groupBy">分组</label>
            <select id="explorer-group-by" data-explorer-control="groupBy">
              <option value="source" data-i18n="shell.explorer.group.source">来源</option>
              <option value="model" data-i18n="shell.explorer.group.model">模型</option>
              <option value="project" data-i18n="shell.explorer.group.project">项目</option>
              <option value="session" data-i18n="shell.explorer.group.session">会话</option>
              <option value="tool" data-i18n="shell.explorer.group.tool">工具</option>
              <option value="tool_kind" data-i18n="shell.explorer.group.toolKind">工具类型</option>
              <option value="is_tool" data-i18n="shell.explorer.group.isTool">工具/非工具</option>
              <option value="token_type" data-i18n="shell.explorer.group.tokenType">Token 类型</option>
            </select>
          </div>
          <div class="filter-group">
            <label for="explorer-granularity" data-i18n="shell.explorer.granularity">粒度</label>
            <select id="explorer-granularity" data-explorer-control="granularity">
              <option value="total" data-i18n="shell.explorer.granularity.total">总计</option>
              <option value="day" data-i18n="shell.explorer.granularity.day">按日</option>
              <option value="week" data-i18n="shell.explorer.granularity.week">按周</option>
              <option value="month" data-i18n="shell.explorer.granularity.month">按月</option>
            </select>
          </div>
          <div class="filter-group">
            <label for="explorer-limit" data-i18n="shell.explorer.limit">Top N</label>
            <input id="explorer-limit" data-explorer-control="limit" type="number" min="1" max="50" step="1" />
          </div>
          <div class="filter-group">
            <label for="explorer-session" data-i18n="shell.explorer.session">会话过滤</label>
            <input id="explorer-session" data-explorer-control="sessionId" type="search" placeholder="session id" data-i18n-attr="placeholder=shell.explorer.sessionPlaceholder" />
          </div>
          <div class="filter-group">
            <label for="explorer-tool-name" data-i18n="shell.explorer.tool">工具过滤</label>
            <input id="explorer-tool-name" data-explorer-control="toolName" type="search" placeholder="Read / Bash / Edit" data-i18n-attr="placeholder=shell.explorer.toolPlaceholder" />
          </div>
          <div class="filter-group">
            <label for="explorer-tool-kind" data-i18n="shell.explorer.toolKind">工具类型</label>
            <select id="explorer-tool-kind" data-explorer-control="toolKind">
              <option value="" data-i18n="shell.explorer.all">全部</option>
              <option value="read">read</option>
              <option value="edit">edit</option>
              <option value="shell">shell</option>
              <option value="mcp">mcp</option>
              <option value="agent">agent</option>
              <option value="(non-tool)">(non-tool)</option>
            </select>
          </div>
          <div class="filter-group">
            <label for="explorer-token-type" data-i18n="shell.explorer.tokenType">Token 类型</label>
            <select id="explorer-token-type" data-explorer-control="tokenType">
              <option value="" data-i18n="shell.explorer.all">全部</option>
              <option value="input">input</option>
              <option value="cache_read">cache_read</option>
              <option value="cache_creation">cache_creation</option>
              <option value="output">output</option>
              <option value="reasoning_output">reasoning_output</option>
            </select>
          </div>
          <label class="explorer-check">
            <input id="explorer-include-other" data-explorer-control="includeOther" type="checkbox" />
            <span data-i18n="shell.explorer.includeOther">合并 Other</span>
          </label>
          <label class="explorer-check">
            <input id="explorer-include-non-tool" data-explorer-control="includeNonTool" type="checkbox" />
            <span data-i18n="shell.explorer.includeNonTool">包含非工具</span>
          </label>
          <div class="explorer-actions">
            <button class="btn btn-primary" id="explorer-apply" type="button" data-i18n="shell.explorer.apply">运行分析</button>
            <button class="btn" id="explorer-reset" type="button" data-i18n="shell.explorer.reset">重置</button>
          </div>
        </div>

        <div id="explorer-summary" class="explorer-summary"></div>
        <div id="explorer-warning"></div>
        <div class="explorer-results-grid">
          <div class="explorer-ranking">
            <div class="panel-title" data-i18n="shell.explorer.rowsTitle">维度排行</div>
            <div class="panel-sub" data-i18n="shell.explorer.rowsSub">按当前指标排序，Top N 之外可合并为 Other。</div>
            <div class="panel-bars" id="explorer-bars"></div>
            <div id="explorer-rows"></div>
          </div>
          <div class="explorer-trends">
            <div class="panel-title" data-i18n="shell.explorer.seriesTitle">时间序列</div>
            <div class="panel-sub" data-i18n="shell.explorer.seriesSub">前 5 个维度使用独立刻度展示，完整区间可用于离线查看。</div>
            <div id="explorer-series-chart"></div>
          </div>
        </div>
        <div id="explorer-series-details"></div>
      </div>
    </section>

    <!-- Cost -->
    <section id="cost" class="block">
      <div class="section-head">
        <div>
          <h2 class="section-title" data-i18n="shell.cost.title">成本估算</h2>
          <div class="section-desc" data-i18n="shell.cost.sub">基于公开计价表的本地估算。仅供参考，与账单存在差异。</div>
        </div>
      </div>

      <div class="cost-status-grid">
        <div class="panel cost-summary-panel">
          <div class="cost-stat-grid" id="costs-stats"></div>
        </div>

        <div class="panel cost-ranking-panel">
          <div class="panel-title" data-i18n="shell.cost.panelTitle">成本最高的 5 个 来源 / 模型 组合</div>
          <div class="panel-sub" data-i18n="shell.cost.panelSub">单位：USD</div>

          <div class="panel-bars" id="costs-bars"></div>
          <div id="costs-table"></div>

          <button class="show-more" type="button" data-toggle-panel="costs" aria-expanded="false" data-i18n="shell.cost.expand">展开全部成本项 →</button>
        </div>

        <div class="panel status-diagnostics-panel" id="status">
          <div class="status-diagnostics-stack">
            <div class="subpanel-section">
              <h3 class="subpanel-title" data-i18n="shell.insights.title">诊断线索</h3>
              <p class="panel-sub" data-i18n="shell.insights.sub">信号只表示可能的下一步，不代表最终诊断。</p>
              <div id="insights-card"></div>
            </div>

            <div class="subpanel-section">
              <h3 class="subpanel-title" data-i18n="shell.failures.title">最近失败</h3>
              <div id="failures-card"></div>
            </div>

            <div class="subpanel-section">
              <h3 class="subpanel-title" data-i18n="shell.integrations.title">集成状态</h3>
              <div id="integrations-rows" class="integration-list"></div>
            </div>
          </div>
        </div>
      </div>
    </section>

    <footer class="foot">
      <span>llmusage v{app_version} · local build</span>
      <span><a href="#overview" data-i18n="shell.footer.backToTop">回到顶部 ↑</a></span>
    </footer>
  </main>
</div>

<script type="module" src="assets/app.js"></script>
</body>
</html>"##,
        mode = mode,
        app_version = app_version,
        supported_sources = supported_sources,
        environment_chip = environment_chip,
        environment_chip_key = environment_chip_key,
        brand_mark = super::brand::BRAND_MARK_SVG
    )
}
