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
    let supported_sources = crate::sources::registered_parsers()
        .into_iter()
        .map(|parser| parser.source().as_str())
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
    if (t === 'dark' || t === 'light') {{
      document.documentElement.setAttribute('data-theme', t);
    }}
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

    <div class="nav-label" data-i18n="shell.nav.label.overview">概览</div>
    <nav>
      <a href="#overview" class="active" data-target="overview">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/></svg></span>
        <span data-i18n="shell.nav.item.usage">用量概览</span>
        <span class="badge">4</span>
      </a>
      <a href="#trends" data-target="trends">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><polyline points="3,17 9,11 13,15 21,7"/><polyline points="14,7 21,7 21,14"/></svg></span>
        <span data-i18n="shell.nav.item.trend">用量趋势</span>
        <span class="badge">24h</span>
      </a>
    </nav>

    <div class="nav-label" data-i18n="shell.nav.label.distribution">分布</div>
    <nav>
      <a href="#models" data-target="models">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2"/></svg></span>
        <span data-i18n="shell.nav.item.models">模型分布</span>
      </a>
      <a href="#sources" data-target="sources">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M12 3l9 5v8l-9 5-9-5V8z"/><path d="M3 8l9 5 9-5M12 13v9"/></svg></span>
        <span data-i18n="shell.nav.item.sources">来源分布</span>
      </a>
      <a href="#projects" data-target="projects">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M3 7h6l2 2h10v11H3z"/></svg></span>
        <span data-i18n="shell.nav.item.projects">项目排行</span>
      </a>
      <a href="#behavior" data-target="behavior">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M4 19V5"/><path d="M4 19h16"/><path d="M8 16v-5"/><path d="M12 16V8"/><path d="M16 16v-7"/></svg></span>
        <span data-i18n="shell.nav.item.behavior">行为分析</span>
      </a>
    </nav>

    <div class="nav-label" data-i18n="shell.nav.label.ops">运营</div>
    <nav>
      <a href="#cost" data-target="cost">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M15 9h-4a2 2 0 100 4h2a2 2 0 110 4H9M12 7v2M12 15v2"/></svg></span>
        <span data-i18n="shell.nav.item.cost">成本估算</span>
      </a>
      <a href="#status" data-target="status">
        <span class="nav-icon"><svg class="i" viewBox="0 0 24 24"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg></span>
        <span data-i18n="shell.nav.item.status">运行状态</span>
      </a>
    </nav>

    <div class="sidebar-footer">
      <div class="sidebar-toggles" role="group" data-i18n-attr="aria-label=toolbar.group.aria">
        <button class="toggle-btn" id="toggle-theme" type="button" data-i18n-attr="aria-label=toolbar.theme.aria">
          <svg class="i toggle-icon icon-sun" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M5 19l1.5-1.5M17.5 6.5L19 5"/></svg>
          <svg class="i toggle-icon icon-moon" viewBox="0 0 24 24"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
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
          <svg class="i" viewBox="0 0 24 24" style="width: 11px; height: 11px;"><path d="M12 22s-8-4.5-8-11a8 8 0 1116 0c0 6.5-8 11-8 11z"/><circle cx="12" cy="11" r="3"/></svg>
          <span data-i18n="{environment_chip_key}">{environment_chip}</span>
        </span>
        <button class="btn" id="btn-export">
          <svg class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3"/></svg>
          <span data-i18n="shell.btn.export">导出 JSON</span>
        </button>
        <div class="refresh-toggle" id="auto-refresh" role="group" data-i18n-attr="aria-label=shell.refresh.aria">
          <span class="refresh-label" data-i18n="shell.refresh.label">刷新</span>
          <button type="button" data-refresh-interval="0" aria-pressed="true" data-i18n="shell.refresh.off">关闭</button>
          <button type="button" data-refresh-interval="30000" aria-pressed="false">30s</button>
          <button type="button" data-refresh-interval="60000" aria-pressed="false">60s</button>
        </div>
        <button class="btn btn-primary" id="btn-sync">
          <svg class="i" viewBox="0 0 24 24" style="width: 13px; height: 13px;"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15A9 9 0 1 1 18 6.36L23 10"/></svg>
          <span data-i18n="shell.btn.sync">同步</span>
        </button>
      </div>
    </div>

    <!-- Hero + Status -->
    <section id="overview" class="block">
      <div class="hero">
        <div>
          <div class="section-eyebrow" style="margin-bottom: 10px;" data-i18n="shell.hero.eyebrow">DASHBOARD</div>
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
        <div class="filter-group">
          <label for="filter-since" data-i18n="shell.filters.since">起始日期</label>
          <input id="filter-since" data-filter="since" type="date" />
        </div>
        <div class="filter-group">
          <label for="filter-until" data-i18n="shell.filters.until">结束日期</label>
          <input id="filter-until" data-filter="until" type="date" />
        </div>
        <div class="filter-actions">
          <button class="btn" id="filters-apply" type="button" data-i18n="shell.filters.apply">应用筛选</button>
          <button class="btn" id="filters-reset" type="button" data-i18n="shell.filters.reset">重置</button>
        </div>
      </div>

      <!-- KPI cards -->
      <div class="kpi-grid" id="kpi-grid"></div>
    </section>

    <!-- Trends -->
    <section id="trends" class="block">
      <div class="trends-card">
        <div class="trends-head">
          <div class="trends-title-block">
            <div class="section-eyebrow" data-i18n="shell.trends.eyebrow">TRENDS</div>
            <div class="trends-title" data-i18n="shell.trends.title">用量趋势</div>
            <div class="trends-sub" data-i18n="shell.trends.sub">主图展示当前窗口内最近 10 条记录，完整明细可展开查看。</div>
          </div>
          <div class="seg" id="seg">
            <button class="active" data-window="day">24h</button>
            <button data-window="week">7d</button>
            <button data-window="month">30d</button>
            <button data-window="all" data-i18n="seg.all">全部</button>
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
          <svg class="chart-svg trends-chart-svg" viewBox="0 0 720 220" preserveAspectRatio="none" id="trends-chart" role="img" aria-label="最近 10 个时段用量趋势">
            <g class="trend-grid-lines">
              <line x1="0" y1="40" x2="720" y2="40"/>
              <line x1="0" y1="80" x2="720" y2="80"/>
              <line x1="0" y1="120" x2="720" y2="120"/>
              <line x1="0" y1="160" x2="720" y2="160"/>
            </g>
            <g id="trends-bars" fill="#c8553d"></g>
            <line class="trend-baseline" x1="0" y1="200" x2="720" y2="200"/>
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
          <div class="section-eyebrow" data-i18n="shell.models.eyebrow">MODELS</div>
          <h2 class="section-title" data-i18n="shell.models.title">模型用量分布</h2>
          <div class="section-desc" data-i18n="shell.models.sub">先看用量最高的模型，再按需展开完整排行。</div>
        </div>
      </div>

      <div class="grid-2">
        <div class="panel">
          <div class="panel-title" data-i18n="shell.models.panelTitle">用量最高的 8 个模型</div>
          <div class="panel-sub" data-i18n="shell.models.panelSub">单位：Token，按累计计算</div>

          <div id="models-bars"></div>
          <div id="models-table"></div>

          <button class="show-more" type="button" data-toggle-panel="models" aria-expanded="false" data-i18n="shell.models.expand">展开完整排行 →</button>
        </div>

        <div>
          <div class="panel" id="sources" style="margin-bottom: 24px;">
            <div class="section-eyebrow" data-i18n="shell.sources.eyebrow">SOURCES</div>
            <div style="display: flex; justify-content: space-between; align-items: baseline;">
              <h3 style="font-size: 18px; font-weight: 600; letter-spacing: -0.018em; margin: 4px 0 4px;" data-i18n="shell.sources.title">来源分布</h3>
              <span class="tag" id="sources-count">--</span>
            </div>
            <div class="panel-sub" data-i18n="shell.sources.sub">用量最高的 4 个来源</div>

            <div class="source-rows" id="sources-rows"></div>
          </div>

          <div class="panel" id="projects">
            <div class="section-eyebrow" data-i18n="shell.projects.eyebrow">PROJECTS</div>
            <div style="display: flex; justify-content: space-between; align-items: baseline;">
              <h3 style="font-size: 18px; font-weight: 600; letter-spacing: -0.018em; margin: 4px 0 4px;" data-i18n="shell.projects.title">项目排行</h3>
              <span class="tag" id="projects-count">--</span>
            </div>
            <div class="panel-sub" data-i18n="shell.projects.sub">按累计 Token 排序</div>

            <div class="project-list" id="projects-rows"></div>

            <button class="show-more" type="button" data-toggle-panel="projects" aria-expanded="false" data-i18n="shell.projects.expand">展开全部项目 →</button>
          </div>
        </div>
      </div>
    </section>

    <!-- Behavior -->
    <section id="behavior" class="block">
      <div class="section-head">
        <div>
          <div class="section-eyebrow" data-i18n="shell.behavior.eyebrow">BEHAVIOR</div>
          <h2 class="section-title" data-i18n="shell.behavior.title">行为分析</h2>
          <div class="section-desc" data-i18n="shell.behavior.sub">基于同步阶段提取的 normalized turn/tool facts；低样本或未支持来源会显式显示降级状态。</div>
        </div>
      </div>

      <div class="grid-2">
        <div class="panel">
          <div style="display: flex; justify-content: space-between; align-items: baseline;">
            <div>
              <div class="panel-title" data-i18n="shell.behavior.activity.title">Activity</div>
              <div class="panel-sub" data-i18n="shell.behavior.activity.sub">按 turn category 聚合 turns、one-shot 与 retry</div>
            </div>
            <span class="tag" id="activity-support">--</span>
          </div>
          <div id="activity-bars" style="margin-top: 18px;"></div>
          <div id="activity-table"></div>
        </div>

        <div class="panel">
          <div style="display: flex; justify-content: space-between; align-items: baseline;">
            <div>
              <div class="panel-title" data-i18n="shell.behavior.tools.title">Tools</div>
              <div class="panel-sub" data-i18n="shell.behavior.tools.sub">Core tools / shell / MCP / agent actions</div>
            </div>
            <span class="tag" id="tools-support">--</span>
          </div>
          <div id="tools-bars" style="margin-top: 18px;"></div>
          <div id="tools-table"></div>
        </div>
      </div>

      <div class="grid-2" style="margin-top: 18px;">
        <div class="panel">
          <div>
            <div class="panel-title" data-i18n="shell.behavior.optimize.title">Optimize</div>
            <div class="panel-sub" data-i18n="shell.behavior.optimize.sub">只读浪费检测；不会自动执行删除、归档或重写。</div>
          </div>
          <div id="optimize-summary" class="mini-stat-grid"></div>
          <div id="optimize-findings" class="finding-list"></div>
        </div>

        <div class="panel">
          <div>
            <div class="panel-title" data-i18n="shell.behavior.compare.title">Compare</div>
            <div class="panel-sub" data-i18n="shell.behavior.compare.sub">按模型对比成本、one-shot、retry 与工作风格；低样本显式提示。</div>
          </div>
          <div id="compare-panel"></div>
        </div>
      </div>
    </section>

    <!-- Cost -->
    <section id="cost" class="block">
      <div class="section-head">
        <div>
          <div class="section-eyebrow" data-i18n="shell.cost.eyebrow">COST</div>
          <h2 class="section-title" data-i18n="shell.cost.title">成本估算</h2>
          <div class="section-desc" data-i18n="shell.cost.sub">基于公开计价表的本地估算。仅供参考，与账单存在差异。</div>
        </div>
      </div>

      <div class="grid-2">
        <div class="panel">
          <div class="panel-title" data-i18n="shell.cost.panelTitle">成本最高的 5 个 来源 / 模型 组合</div>
          <div class="panel-sub" data-i18n="shell.cost.panelSub">单位：USD</div>

          <div id="costs-bars" style="margin-top: 18px;"></div>
          <div id="costs-table"></div>

          <button class="show-more" type="button" data-toggle-panel="costs" aria-expanded="false" data-i18n="shell.cost.expand">展开全部成本项 →</button>
        </div>

        <div id="status" class="block" style="margin: 0;">
          <div class="panel">
            <div class="cost-grid" id="costs-stats"></div>

            <div style="border-top: 1px dashed var(--line); padding-top: 18px;">
              <div class="section-eyebrow" data-i18n="shell.insights.eyebrow">INSIGHTS</div>
              <h3 style="font-size: 16px; font-weight: 600; margin: 4px 0 6px;" data-i18n="shell.insights.title">诊断线索</h3>
              <p class="panel-sub" data-i18n="shell.insights.sub">信号只表示可能的下一步，不代表最终诊断。</p>
              <div id="insights-card"></div>
            </div>

            <div style="border-top: 1px dashed var(--line); padding-top: 18px;">
              <div class="section-eyebrow" data-i18n="shell.failures.eyebrow">FAILURES</div>
              <h3 style="font-size: 16px; font-weight: 600; margin: 4px 0 12px;" data-i18n="shell.failures.title">最近失败</h3>
              <div id="failures-card"></div>
            </div>

            <div style="border-top: 1px dashed var(--line); padding-top: 18px; margin-top: 18px;">
              <div class="section-eyebrow" data-i18n="shell.integrations.eyebrow">INTEGRATIONS</div>
              <h3 style="font-size: 16px; font-weight: 600; margin: 4px 0 12px;" data-i18n="shell.integrations.title">集成状态</h3>
              <div id="integrations-rows" style="display: grid; gap: 10px;"></div>
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
