# llmusage Web 看板现状清单

来源：Explore 代理对本仓库 `src/web/`、`src/query/`、`src/commands/serve.rs` 的深度扫描（2026-08-03）。

## 1. 服务端

- 入口：`llmusage serve` → `src/commands/serve.rs:28` → `web::bind_server`（`src/web/mod.rs:431`）。axum + tokio + gzip/br 压缩。端口探测 37421→37422→37423→0，默认 `127.0.0.1`，`--public` 时 `0.0.0.0` 且只挂载只读子集（`public_router`，`src/web/mod.rs:520`）。
- 只读端点（loopback 全集）：`/api/dashboard`、`/api/overview`、`/api/trends`、`/api/trends_daily`、`/api/models`、`/api/sources`、`/api/projects`、`/api/costs`、`/api/activity`、`/api/tools`、`/api/explorer`、`/api/optimize`、`/api/compare/models`、`/api/compare`、`/api/home_overview`、`/api/heatmap`、`/api/logs`、`/api/diagnostics`、`/api/health`、`/api/jobs/{id}`。
- 写端点（仅 loopback + peer-IP 校验）：`POST /api/jobs`、`POST /api/jobs/{id}/cancel`、`POST /api/diagnostics/forget`。
- `/api/dashboard` 支持 `scope=full|interactive` + `window=`，由 `load_dashboard_snapshot_resilient`（`src/web/mod.rs:1586`）分节降级。防护：4 permit 查询信号量、5s API 超时（行为类 3s）、1.5s SQLite busy timeout、30s 诊断缓存。

### 已实现已路由但前端零调用（本次优化的最大红利）

| 端点                  | 返回                                                                                                                                      | 用途                    |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| `/api/heatmap`        | `Vec<HeatmapPoint{date,event_count,total_tokens}>`，零填充连续网格，最多 366 天，DST 正确（`src/query/heatmap.rs`）                       | GitHub 式贡献日历       |
| `/api/logs`           | 游标分页 `LogRecord`：完整 token 拆分、单事件成本、定价状态、项目标签、**session_id/session_label**、可选 raw_json（`src/query/logs.rs`） | 事件日志/会话浏览       |
| `/api/trends_daily`   | `DailyTrendPoint`：每日 input/cache_read/cache_creation/output 拆分 + 日成本                                                              | 多序列/堆叠日趋势       |
| `/api/home_overview`  | sessions/requests/tokens/cost/cache 效率/active days/平台数总计 + 按平台 map + 按平台日序列（`src/query/home_overview.rs`）               | AgentsView 式汇总统计卡 |
| `/api/compare/models` | 模型候选列表                                                                                                                              | 模型选择器              |

这些端点原为外部 `ccr-ui` 消费者规格化（`docs/prd/llmusage-integration-prd-v1.1.md:630-634`）。

### 在 query 层但无 HTTP 路由（需加路由才能用）

- `context_pressure`（各模型上下文窗占用峰值/均值）
- `blocks_report`（5 小时计费块 + burn rate）
- `src/query/reports.rs` 家族：`load_session_report` / `load_single_session_report` — 每会话行含 span 分钟、活跃分钟、模型拆分、项目归属。**Top Sessions 只缺路由 + UI**。

## 2. 前端结构

- 纯 vanilla ES modules，无框架无构建。26 个资产经 `include_str!` 内嵌（清单 `src/web/assets/mod.rs:70`，类型是 `[WebAsset; 26]`，新增文件必须同步改计数）。同一清单同时供 `serve` 与 `export html`（`src/export/mod.rs:22`）。
- HTML shell 是 Rust format 字符串：`src/web/shell.rs:22`。单页文档，全部 section 常驻 DOM，`data-mode="live"|"snapshot"`。
- **不是多页/多 tab**：单条长页 + 侧栏 `#anchor` 跳转（`setupNavigation`，`app.js:732`）。无客户端路由。
- 渲染用指纹缓存（`data/render-key.js`）避免未变面板重写；加载渐进：核心 `/api/dashboard` 先到，Activity/Tools/Optimize/Compare/Explorer 并发 2 独立加载。

| 层     | 文件                                                                                                              |
| ------ | ----------------------------------------------------------------------------------------------------------------- |
| CSS    | `base.css`（token）、`layout.css`、`components.css`（2433 行）、`charts.css`                                      |
| 核心   | `app.js`（1823 行）、`data.js`、`data/{fetch,derive,format,render-key}.js`                                        |
| 渲染器 | `render/{hero,trends,models,sources,projects,costs,behavior,explorer,insights,sync-command-center}.js`            |
| 基建   | `copy.js`（1088 行 zh/en）、`i18n.js`、`theme.js`、`runtime.js`、`load-state.js`、`bootstrap-watchdog.js`（内联） |

## 3. 现有版面与部件

六个锚点 section（`src/web/shell.rs:186-539`）：

1. **Overview**：hero 元信息、可折叠运行状态、过滤栏、4 张 KPI 卡（总 token/24h token/活跃源数/总成本，`data/derive.js:462`）、Sync Command Center（454 行，任务状态/进度/取消，900ms 轮询）。
2. **Trends**：手写 SVG 柱图**只显示最近 10 桶**（`render/trends.js`），上方 3 张统计卡，下方全量表 + 分源表。窗口段控件 24h/7d/30d/all。
3. **Distribution**：Models top-8 横条 + 表、Sources top-4、Projects 排名列表。
4. **Behavior**：Activity/Tools/Optimize/Compare 四面板（基于 usage_turn/usage_tool_call），每个带支持度/降级 chip。
5. **Cost Explorer**：11 控件的切片工作台（指标/分组/粒度/TopN/过滤...），结果为排名条 + top5 sparkline。
6. **Cost**：成本统计卡、top-5 源/模型成本排名、诊断面板。

全局控件：源下拉、模型文本框、1d/7d/30d/all + 自定义 since/until（手写日期弹层 `app.js:966-1108`）、自动刷新（off/30s/60s）、Export JSON、Sync 按钮。过滤状态经 URL query 往返。

主题：Catppuccin Latte/Mocha 双主题（`base.css:10`/`:72`），语义 token `--bg/--surface/--ink/--accent(mauve)/--data-accent(peach)` 等，`color-mix(in oklab)` 派生。字体系统栈 + `tabular-nums`。布局：固定 248px 左侧栏 + 滚动主栏，断点 1450/1100/720px。无障碍：`:focus-visible` 环、aria-pressed/labelledby/live、reduced-motion。

本地化：完整 zh/en 切换，shell 所有字符串带 `data-i18n*`。

**缺失**：全局搜索、会话浏览器、会话/消息详情、原始日志查看器、日历热力图、CSV 导出、饼图、主趋势区多序列折线（仅 Explorer sparkline）。

## 4. 数据模型

- 读门面 `Dashboard`（`src/query/mod.rs:888`），每快照一个 SQLite 连接。表：`usage_event`（含 session_id/session_label/project_hash）、`usage_bucket_30m`（聚合主力，source+model+hour_start+project_hash）、`project_dim`、`usage_turn`、`usage_tool_call`、`run_log`、`source_*`。
- 所有查询共享 `QueryFilter`（source/model/since/until/project_hash/timezone）。
- 注册源（`src/domain/source_descriptor.rs`）：codex、claude、opencode、antigravity（仅历史）、kimi_code、pi、grok。**不支持 gemini**。
- llmusage **不存消息正文** — 转录查看、内容搜索、Recall 类功能没有数据基础。

## 5. 文档与约束

- 权威文档：`docs/dashboard/index.md`（+ `docs/zh/dashboard/index.md`），169 行，含降级词汇表（no_data/degraded/insufficient_models/low_sample/unsupported）、截图夹具 `cargo run --features testing --example docs_dashboard_serve -- --port 37421`（1440×1100）。
- 相关 ADR：0005（内存任务注册表）、0008（源能力注册表）、0010（provider 标签维度）。
- CI 约束（`justfile:61-65`）：`node --check` + `node --test`（`scripts/tests/dashboard-{fetch,bootstrap-watchdog,load-state,render-lifecycle}.test.mjs` 直接 import 真实资产模块——改 `data/fetch.js`/`load-state.js`/渲染生命周期的导出形状会破坏 CI）。
- JS 单引号风格；全局 prettier hook 会改坏——**JS/CSS 编辑走 Bash（heredoc/sed），不要用 Edit 工具**（见用户记忆）。
- 新资产文件必须注册进 `ASSET_MANIFEST`（`src/web/assets/mod.rs:70`）否则 serve 与 export html 双双 404。

## 6. 关键路径速查

| 用途               | 路径                                        |
| ------------------ | ------------------------------------------- |
| 服务绑定 + 路由    | `src/web/mod.rs:431`、`:514-551`            |
| HTML shell         | `src/web/shell.rs:22`                       |
| 资产清单           | `src/web/assets/mod.rs:70`                  |
| 前端入口           | `src/web/assets/app.js:83`                  |
| fetch 层           | `src/web/assets/data/fetch.js:201`          |
| KPI 派生           | `src/web/assets/data/derive.js:462`         |
| 颜色 token         | `src/web/assets/base.css:10`                |
| query 门面         | `src/query/mod.rs:888`                      |
| 已就绪未用查询     | `src/query/{heatmap,logs,home_overview}.rs` |
| 会话报表（待路由） | `src/query/reports.rs`                      |
| 静态导出           | `src/export/mod.rs:22`                      |
