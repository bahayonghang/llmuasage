# Implement：接入已就绪端点

前置阅读：本任务 `prd.md`、`design.md`；父任务 `research/review-verification.md`（真实契约，已核验，无需重复核对字段名）。前置任务 `08-03-dashboard-visual-system` 必须已合入（`.dash-grid`、`.chart-tooltip`、`--chart-cat-*`、`--hm-l*`）。

> ⚠️ JS/CSS 修改一律走 Bash；Rust（shell.rs、assets/mod.rs、query/mod.rs、export）可用 Edit。Rust 提交前跑 `cargo fmt`。

## 步骤

### 0. 基线摸底（轻量，非契约核对）

- [ ] 跑 `node --test scripts/tests/` 全套记录基线；列出 `SECONDARY_SECTIONS`/`secondaryTotal` 相关断言清单。
- [ ] `curl` 实拍三端点响应样本存本任务 `research/api-samples.md`（字段名已在 review-verification §6 固化，此处只取样例值供渲染器开发）。

### 1. 后端：快照 DTO 扩展

- [ ] `DashboardSnapshot` 三个 `Option` 字段（home_overview 裁剪投影 summary+by_platform、heatmap 366 天、trends_daily）+ `snapshot()` 组装。
- [ ] Rust 序列化测试：新快照含三键；反序列化旧样例（手工构造缺键 JSON）不失败。
- 验证：`cargo test -- --test-threads=1` 相关测试 + `cargo clippy`。

### 2. 加载生命周期扩容

- [ ] `load-state.js`：`SECONDARY_SECTIONS` +3；`app.js`：loaders 表 +3（generation 守卫内）。
- [ ] `dashboard-load-state.test.mjs` / `dashboard-render-lifecycle.test.mjs`：更新受影响断言 + 新增 stale 丢弃、进度完成时机用例（每个新 section 至少一条）。
- [ ] `data/fetch.js` 三个 fetch 函数 + `dashboard-fetch.test.mjs` 三条用例。
- 验证：`node --test scripts/tests/` 全绿。

### 3. SummaryCards

- [ ] `render/summary-cards.js` + `derive.js::buildSummaryCards`（prd R1 映射表）+ shell.rs 容器替换 + `hero.js`/`buildKpis` 退役清孤儿 + copy 键 + manifest +1。
- 验证：与 `/api/home_overview` JSON 对表；空库空态；`node --test`。

### 4. CalendarHeatmap

- [ ] `render/calendar-heatmap.js`（SVG 几何、`heatmapLevels` P25/50/75 纯函数进 derive.js、点击下钻/恢复、tooltip、指标切换）+ shell.rs `.wide` 容器 + copy 键 + manifest +1。
- [ ] `heatmapLevels` node 纯函数用例（全零、单值、分位边界）。
- 验证：双主题标尺、下钻联动、366 天宽度滚动。

### 5. TrendsDaily

- [ ] `render/trends-daily.js`（堆叠柱 + niceScale + 图例 + tooltip）+ shell.rs 容器 + copy 键 + manifest +1（终态 `[WebAsset; 29]`）。
- 验证：与原始 JSON 对表 3 天；24h 窗口空态。

### 6. 快照前端接通

- [ ] `data.js` 快照读取路径接三键；缺键 → 空态（node 用例一条）。
- 验证：`cargo run -- export html` 产物离线打开三部件正常；手工删键的旧格式快照不报错。

### 7. 性能与收尾

- [x] `src/store/migrations.rs` 增加 schema v20，且只创建 `idx_usage_event_home_compact_cover`。
- [x] migration 测试：v19→v20、fresh→v20、精确表达式/列顺序、无 identity-first 索引、all/date-range covering plan。
- [x] compact/full 等价测试改为整数/map/结构精确、`f64` 绝对误差 ≤ `EPSILON = 1e-9`；覆盖 source/model/project/date/IANA 组合过滤与 HTTP 边界。
- [ ] `curl -w '%{size_download} %{time_total}'` 实测三端点（热身后），记录进本任务 research；核对 ≤ 400ms / 快照总量 ≤ 128 KiB 预算。
- [ ] 确认 home_overview 80ms 种子测试在 CI 结果中无回归。
- [ ] docs 双语三部件章节。
- [ ] `just ci` 全绿。

## 回滚点

步骤 1（后端快照）、2（生命周期）各一提交；部件 3/4/5 各一提交；revert 单提交可摘除单部件（生命周期扩容行随对应部件提交回滚时需同步缩容——回滚说明写进提交信息）。

schema v20 无数据转换：同版本 binary 可 `DROP INDEX idx_usage_event_home_compact_cover` 回收索引；binary downgrade 必须恢复迁移前备份，使 `schema_version` 与旧 binary 匹配，禁止只手改版本号。

## 提交建议

`feat(看板): [AI] ✨ 接入 home_overview 汇总统计卡` 等，每步一条。
