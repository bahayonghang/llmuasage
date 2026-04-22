/*
 * ========================================================================
 * 步骤1：生成 live / snapshot 共用页面骨架
 * ========================================================================
 * 目标：
 * 1) 把首屏结构固定成英雄区、概览卡、KPI、趋势区和分析区
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
        "离线文件 · 静态导出"
    } else {
        "127.0.0.1 · 本地服务"
    };
    let hero_subhead = if mode == "snapshot" {
        "查看 Codex、Claude、OpenCode 的本地 Token 用量、模型分布、项目排行、成本估算和运行状态。"
    } else {
        "本地查看近期用量、成本估算和运行状态，不改变现有数据接口。"
    };

    format!(
        r##"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>llmusage 本地用量概览</title>
    <link rel="stylesheet" href="assets/base.css" />
    <link rel="stylesheet" href="assets/layout.css" />
    <link rel="stylesheet" href="assets/components.css" />
    <link rel="stylesheet" href="assets/charts.css" />
  </head>
  <body data-mode="{mode}">
    <main class="page-shell">
      <section class="hero-stage">
        <div class="hero-main">
          <div class="hero-copy">
            <div class="hero-kicker-row">
              <p class="hero-kicker">仅本地</p>
              <span class="hero-chip">{environment_chip}</span>
            </div>
            <h1>llmusage 本地用量概览</h1>
            <p class="hero-subhead">
              {hero_subhead}
            </p>
          </div>
          <section id="overview" class="overview-grid"></section>
        </div>
        <aside id="ledger-summary" class="ledger-card hero-summary"></aside>
      </section>

      <section class="trend-stage">
        <div class="section-header section-header--stage">
          <div>
            <p class="section-kicker">趋势</p>
            <h2>用量趋势</h2>
            <p class="section-copy">主图展示当前窗口内最近 10 条记录，完整明细可展开查看。</p>
          </div>
          <div class="window-switch">
            <button type="button" data-window="day">24h</button>
            <button type="button" data-window="week">7d</button>
            <button type="button" data-window="month">30d</button>
            <button type="button" data-window="all">全部</button>
          </div>
        </div>
        <div id="trend-spotlight" class="trend-spotlight"></div>
        <div id="trend-ledger"></div>
      </section>

      <section class="analysis-grid">
        <div class="analysis-column analysis-column--main">
          <section class="panel panel--feature">
            <div class="section-header section-header--tight">
              <div>
                <p class="section-kicker">模型</p>
                <h2>模型用量分布</h2>
                <p class="section-copy">先看用量最高的模型，再按需展开完整排行。</p>
              </div>
            </div>
            <div class="panel-stack">
              <div id="models-chart"></div>
              <div id="models-table"></div>
              <div id="models-ledger"></div>
            </div>
          </section>

          <section class="panel">
            <div class="section-header section-header--tight">
              <div>
                <p class="section-kicker">成本</p>
                <h2>成本估算</h2>
              </div>
            </div>
            <div class="panel-stack panel-stack--compact">
              <div id="costs-chart"></div>
              <div id="costs-table"></div>
              <div id="costs-ledger"></div>
            </div>
          </section>
        </div>

        <div class="analysis-column analysis-column--rail">
          <section class="panel">
            <div class="section-header section-header--tight">
              <div>
                <p class="section-kicker">来源</p>
                <h2>来源分布</h2>
              </div>
            </div>
            <div class="panel-stack panel-stack--compact">
              <div id="sources-chart"></div>
              <div id="sources-table"></div>
              <div id="sources-ledger"></div>
            </div>
          </section>

          <section class="panel">
            <div class="section-header section-header--tight">
              <div>
                <p class="section-kicker">项目</p>
                <h2>项目排行</h2>
              </div>
            </div>
            <div class="panel-stack panel-stack--compact">
              <div id="projects-table"></div>
              <div id="projects-ledger"></div>
            </div>
          </section>

          <section class="panel panel--dark panel--health">
            <div class="section-header section-header--tight">
              <div>
                <p class="section-kicker">状态</p>
                <h2>运行状态</h2>
              </div>
            </div>
            <div id="health"></div>
          </section>
        </div>
      </section>
    </main>
    <script type="module" src="assets/app.js"></script>
  </body>
</html>"##
    )
}
