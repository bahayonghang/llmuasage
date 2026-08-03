# Design：接入已就绪端点

真实契约依据：父任务 `research/review-verification.md`（§3 快照 DTO、§4 加载生命周期、§6 home_overview 字段、§7 预算）。

## 数据流：并入 latest-wins 生命周期（不是独立 fetch）

`load-state.js` 的 `SECONDARY_SECTIONS` 从 5 扩到 8：

```
['activity','tools','optimize','explorer','compare','home_overview','heatmap','trends_daily']
```

- `app.js` `loadDashboardProgressive` 的 loaders 表加三个 loader（走既有 `runLoadersWithConcurrency` 并发 2、generation 守卫、AbortController、`secondaryLoadingPayload` 占位）。
- 面板渲染沿用指纹缓存（`panelFingerprint` 各自键）。
- 注意 `SECONDARY_SECTIONS` 是 `Object.freeze` 导出常量且被 node 测试 import —— 扩容即导出值变化，`dashboard-load-state.test.mjs` 中依赖 `secondaryTotal=5` 的断言必须同步更新；这是**预期内的测试更新**，不属于"破坏导出形状"。

### fetch 层

`data/fetch.js` 追加 `fetchHomeOverview / fetchHeatmap / fetchTrendsDaily`（复用现有 QueryFilter 序列化助手；timezone 参数由 timezone-iana 任务落地后自动携带）。`dashboard-fetch.test.mjs` 按既有模式补三条用例。

## 部件设计

### SummaryCards（`render/summary-cards.js`）

纯 DOM。字段映射按 prd R1 表（真实字段名，已核验）。`data/derive.js` 加 `buildSummaryCards(homeOverview)`；`buildKpis`（`derive.js:462`）及 `render/hero.js` KPI 路径退役，清理仅被其引用的孤儿 helper（共享 helper 不动）。shell.rs KPI 容器换六卡容器。

### CalendarHeatmap（`render/calendar-heatmap.js`）

内联 SVG，几何常量照 AgentsView（CELL 16 / GAP 2 / LABEL_W 36 / HEADER_H 16 / rx 2 / 高 146）。宽 = 周数 × 18 + 36，容器横向滚动。

- 颜色：`.hm-l0..l4` 类（token 由 visual-system 定义，本任务只消费）。
- 分档：非零值 P25/50/75 三分位 → level 1–4，零 → level 0（`data/derive.js` 纯函数 `heatmapLevels`，node 可测）。
- 交互：`click` 委托 `<rect data-date>`；下钻 = 现有过滤 apply 设 since=until=该日，先前范围存模块级变量，再点恢复；选中格 `stroke: var(--text-primary); stroke-width:2`。
- tooltip：单例 `.chart-tooltip` div，fixed 定位 mousemove 跟随。
- 指标切换 Tokens/Events：`.seg` 控件，切换只重算 level 不重新 fetch。

### TrendsDaily（`render/trends-daily.js`）

堆叠柱 SVG：高 180 + 轴；`niceScale()`（1/2/5×10ⁿ）内部函数；四序列 `--chart-cat-1..4` 类色；tooltip 显示日期、四类 token、日成本；图例 8px 圆点。range=24h 空态（copy 键）。

## 快照导出（后端）

`DashboardSnapshot` 新增：

```rust
/// Home overview projection for the summary card row. `None` in snapshots
/// created before this field existed.
pub home_overview: Option<HomeOverviewPayload 投影>,
pub heatmap: Option<Vec<HeatmapPoint>>,
pub trends_daily: Option<Vec<DailyTrendPoint>>,
```

- `Dashboard::snapshot()` 组装时填 `Some(...)`（heatmap 取 366 天）；序列化保持字段常在（`Option` 仅为旧快照反序列化兼容 —— 前端读不到键时走空态）。
- home_overview 投影裁剪：快照只需 `summary` + `by_platform`（bootstrap/archive 是 ccr-ui 专用，不进快照）—— 若直接复用 `HomeOverviewPayload` 更省事且体积可接受，实现时以 128 KiB 快照预算为准取舍，design 倾向裁剪投影。
- 测试：Rust 侧 snapshot 序列化含新键断言；前端 `data.js` 快照读取路径对缺键快照返回空态（node 用例）。

## 性能

- 三个 loader 并发 2 排队，不与核心 `/api/dashboard` 抢首屏；`CORE_SLOW_MS/CORE_TIMEOUT_MS` 不变。
- heatmap 366 天 ≈ 366 行、trends_daily ≤ 366 行、home_overview 单对象 —— 载荷远小于 128 KiB；验收时用 `curl -w '%{size_download} %{time_total}'` 实测记录。
- home_overview 80ms 种子测试（`dashboard-performance-contracts.md:492`）在 CI 已存在，不得回归。

### schema v20 covering index

compact home overview 保持一次流式 `usage_event` 扫描。v20 只增加一个与投影顺序一致的 covering expression index：

```sql
CREATE INDEX idx_usage_event_home_compact_cover
ON usage_event(
    event_at,
    source,
    model,
    project_hash,
    COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key),
    input_tokens,
    cache_creation_tokens,
    cache_read_tokens,
    total_tokens,
    cost_with_cache_usd
);
```

- `event_at` 首列同时覆盖 all-range scan 与日期范围 search；source/model/project 过滤仍保留现有索引供 planner 选择。
- 不增加 identity-first 第二索引：它额外占用约 31.70 MiB，虽能去掉 session distinct 临时 B-tree，但不能解决被拒绝的 exact-cost rescan 瓶颈。
- 不增加 all-range table-order cost rescan：它把 all-range 从 222–240ms 拉回 673–770ms，仅消除 `1.8189894035458565e-11` 的浮点求和顺序差异。
- 等价契约：整数、map 键、结构精确；`f64` 绝对误差 ≤ `EPSILON = 1e-9`。
- rollback：正常回滚为 `DROP INDEX idx_usage_event_home_compact_cover`；若旧 binary 不能接受 v20 schema version，则恢复升级前备份。无表数据转换。

## 资产清单

新增 3 个 JS → `ASSET_MANIFEST` +3，`[WebAsset; 26]` → `[WebAsset; 29]`。

## 错误与降级

- 各 loader 失败 → 该 section 降级条（现有 load-state 语义），不影响其他面板。
- range=all 时 heatmap 请求 366 天并在标题注明"最近一年"。

## 风险

1. `SECONDARY_SECTIONS` 扩容牵连的 node 断言面未知 —— 实现步骤先跑一次现有测试列出受影响断言再改。
2. 替代 KPI 行触碰 `derive.js`/`hero.js` —— 只清理孤儿，共享 helper（格式化函数等）保留。
3. 快照体积：若复用完整 `HomeOverviewPayload` 导致快照含诊断大对象，改裁剪投影（design 默认裁剪）。
