# PRD：接入已就绪端点 —— 汇总卡 / 贡献日历 / 每日趋势

父任务：`.trellis/tasks/08-03-dashboard-agentsview-alignment`。前置依赖：`08-03-dashboard-visual-system` 已完成（token 契约 `--chart-cat-*`/`--hm-l*`/`--source-*` 与 `.dash-grid` 骨架就位）。真实契约事实：父任务 `research/review-verification.md`（§3 §4 §6 §7 为本任务权威依据）。

## Goal

把三个已实现、已路由、但前端零调用的端点接入看板，补齐 AgentsView 三大核心部件：汇总统计卡行、GitHub 式贡献日历、每日多序列趋势。

**范围修正声明**（替代原"零后端改动"）：无新 HTTP 端点，但包含三类必要基建改动 —— ① `DashboardSnapshot` DTO 扩展（快照导出承载新部件数据）；② 新面板并入 latest-wins 加载生命周期；③ schema v20 新增一个 event-at-first covering expression index，使代表性 19 万事件库的 compact home overview 满足交互预算。v20 不改表、写入路径或业务语义，也不引入第二个 identity-first 索引。

## Requirements

### R1 汇总统计卡行（数据源 `GET /api/home_overview`）

真实字段（review-verification §6，无需再核对）：`summary.{total_sessions,total_requests,total_tokens,total_cost_usd,cache_efficiency,active_days,platforms}` + `by_platform`（全源覆盖的 BTreeMap）。**不使用 `series`**（固定 claude/codex/antigravity/opencode 四键，kimi_code/pi/grok 缺失）。

六卡映射（**已决策，无"实现时二选一"**）：

| 卡 | 主值 | 副标签 |
|---|---|---|
| Sessions | summary.total_sessions | `{summary.platforms}` 个平台 |
| Requests | summary.total_requests | 均值 requests/session（前端算） |
| Tokens（featured，2px 蓝边） | summary.total_tokens | by_platform 中 tokens 最高平台名 |
| Cost | summary.total_cost_usd | 无副标签（成本细分看 Cost 区） |
| Active Days | summary.active_days | 当前范围天数 |
| Cache 效率 | summary.cache_efficiency（%） | 固定说明文案（zh/en） |

新六卡行**替代**现有 4 张 KPI 卡；原 KPI 的"last-24h tokens"信息随 Trends 区保留的 24h 窗口可查，不并入卡片（避免双数据源一行）。`render/hero.js` 的 KPI 渲染路径退役并清理孤儿代码。

### R2 GitHub 式贡献日历（数据源 `GET /api/heatmap`）

- `.wide` 首位；几何与配色按 AgentsView 规格（CELL 16/gap 2/rx 2/高 146、月与星期标签规则）；颜色用 visual-system 定义的 `--hm-l0..l4`。
- 指标切换 Tokens / Events（`HeatmapPoint.total_tokens`/`event_count`）；level 前端按非零值 P25/50/75 分档。
- 点击某天：全局过滤收缩到该日；再点恢复先前范围。tooltip 用 `.chart-tooltip`。
- 天数随当前 range（默认一年，上限 366）。

### R3 每日多序列趋势（数据源 `GET /api/trends_daily`）

- 每日堆叠柱：input / cache_read / cache_creation / output 四序列（色 `--chart-cat-1..4`），日成本进 tooltip（不做双轴）。
- 高 180、`niceScale` 1/2/5×10ⁿ 刻度、8px 圆点图例。
- 现有 10 桶柱图保留（服务 24h 粒度）；range=24h 时新图显示空态说明。

### R4 加载生命周期（review-verification §4，必做）

- 三个新面板注册进 `SECONDARY_SECTIONS`（`load-state.js:1` 冻结数组扩容）并走 `loadDashboardProgressive` 的 generation 守卫 + AbortController——**禁止独立裸 fetch**。
- `secondaryLoadingPayload` 占位、进度计数（`secondaryTotal`）随之正确。
- `dashboard-load-state.test.mjs`、`dashboard-render-lifecycle.test.mjs` 同步扩展：新 section 的 stale-response 丢弃、进度完成时机各至少一条用例。

### R5 快照导出（review-verification §3，必做后端）

- `DashboardSnapshot`（`src/query/mod.rs:798`）新增三个 `Option` 字段（home_overview/heatmap/trends_daily 投影）+ `Dashboard::snapshot()` 组装。
- 兼容：旧 snapshot.json（缺新键）→ 前端空态不报错；Rust 侧字段 `#[serde(skip_serializing_if)]` 或恒 Some 二选一在 design 定。
- 快照兼容测试：Rust 序列化测试 + 前端 node 测试各一条。

### R6 通用

- 新字符串进 `copy.js` zh/en；新 fetch 走 `data/fetch.js` 追加导出；新渲染器文件登记 `ASSET_MANIFEST`（`[WebAsset; 26]` → 29）。

### R7 compact home overview schema 性能

- schema v20 仅创建 `idx_usage_event_home_compact_cover`，列序为 `event_at, source, model, project_hash, session identity expression, input_tokens, cache_creation_tokens, cache_read_tokens, total_tokens, cost_with_cache_usd`。
- session identity expression 固定为 `COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key)`，与 compact 查询一致；不得增加 identity-first 第二索引或 all-range 成本重扫。
- compact/full 对照中，整数、map 键与结构必须精确一致；`f64` 字段允许绝对误差不超过既有 `EPSILON = 1e-9`。索引顺序导致的代表性库 all-range cost 差值 `1.8189894035458565e-11` 在该契约内。
- migration 测试覆盖 v19 → v20、fresh → v20、精确 DDL、无第二索引，以及 all/date-range compact projection 使用 covering index。

## 非目标

- 不做 hour-of-week、Top Sessions、CSV（子任务 session-analytics）。
- 不新增 HTTP 端点、不动路由、不动 `/api/dashboard` 交互 scope。
- 不改同步/解析写路径，不新增 identity-first 索引，不为 byte-level 浮点顺序一致性增加第二次表扫描。

## Acceptance Criteria

- [ ] 三部件随全局过滤正确刷新；快速连续切换过滤/范围无旧数据覆盖（latest-wins 手工验证 + 新增 node 用例通过）。
- [ ] 贡献日历双主题绿标尺正确、点击下钻/恢复正确。
- [ ] 汇总卡数值与 `/api/home_overview` 原始 JSON 抽查一致；空库显式空态。
- [ ] **性能**（review-verification §7）：三个端点响应各 ≤ 400ms（热身后、代表性本地库实测记录数值）；home_overview 80ms×3 种子测试不回归；新增面板不推迟核心 KPI 首屏。
- [ ] schema v20 在 fixture/temp DB 上验证；代表性备份的 compact `1d/7d/30d/all` 热身后三次均 ≤ 400ms，且 real user DB 未被迁移或写入。
- [ ] 快照：`export html` 产物离线打开三部件正常；**旧版 snapshot.json 加载不报错**（兼容测试通过）。
- [ ] zh/en 双语完整；`just ci` 全绿。
- [ ] `docs/dashboard/index.md` + `docs/zh/dashboard/index.md` 新增三部件章节。
