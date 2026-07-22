# S2 渲染生命周期：before/after 插桩证据

任务：`.trellis/tasks/07-22-serve-dash-render-lifecycle`（PRD R2.1–R2.6 / 验收 A2.1–A2.7）。
佐证方式：node 插桩计数（`buildContextStats` / `numberFormatterStats` / 指纹比对）+ 代码走读；非真实浏览器数据。

## 1. buildContext 派生次数（A2.2 / A2.3）

复跑命令：`node .trellis/tasks/07-22-serve-dash-render-lifecycle/research/instrumentation-counts.mjs`
（同目录 `instrumentation-counts.mjs`，直接 import `src/web/assets/data/derive.js` 与 `data/fingerprint.js`，按改造后 app.js 的调用序列驱动。）

实测输出（2026-07-22，本工作树）：

| 场景 | 旧路径真算 | 新路径调用 | 新路径真算 | 说明 |
| --- | --- | --- | --- | --- |
| renderDashboard 单次调用链 | 2 | 3 | 1 | 三个渲染入口共享同引用 memo |
| fast-range 全流程（10 个调用点） | 10 | 10 | 8 | 每次真算都对应一次真实数据到达（refreshing 标记、core、5 个 secondary、settle） |
| locale 切换重渲 | 2 | 3 | 0 | memo 命中；指纹 key 含 locale，文案面板指纹自然失效并正常重渲 |
| 展开/折叠单个面板 | 2 | 1 | 0 | memo 命中；仅该面板 extra(expanded) 指纹变化 |
| 自动刷新 tick（数据未变） | 2 | 0 | 0 | `dashboardFingerprint` 短路，见第 3 节 |
| job 轮询无变化 tick（900ms） | 1 | 1 | 0 | memo 命中 + 浅比对提前跳过 |

口径说明：A2.3 "buildContext 调用次数从 ~7 降为 1" 按审定方案 D2.3 解读为"同一份数据（同引用）的重复派生降为 1 次"——renderDashboard 单次调用链从 2 次真算降为 1 次；fast-range 流程中剩余的 8 次真算每一次都对应一次新数据到达（rawData 引用替换），属于必要派生。design.md 现状链中"~7 次"正是对同一 rawData 的重复派生（renderDashboard 内 2 次、fast-range 开始/settle 各 2 次同引用调用、job 轮询每 tick 1 次），这些已全部 memo 命中。

## 2. Intl.NumberFormat 构造次数（A2.3）

`data/format.js` 改为模块级小 Map（key = `locale|JSON.stringify(options)`，调用点固定 3 种组合）。
node 测试断言：1000 次格式化调用后 `constructed = 3 / cached = 3`，预热后第二轮 500 次调用构造数零增长（`scripts/tests/dashboard-render-lifecycle.test.mjs` 的 "Intl.NumberFormat construction is bounded"）。

## 3. 自动刷新 / sync 完成：DOM 写入策略变化（A2.1 / A2.5）

before：`scheduleAutoRefresh` tick → `refreshDashboardInPlace` → `reloadDashboard`（**full scope** 请求）→ `renderDashboard` 无条件全量 `innerHTML` 重建；sync 完成同样走 `reloadDashboard`。

after：两者改走 `reloadDashboardAfterDataChange` → 既有 `reloadDashboardFastRange`（**scope=interactive**，复用 fast-range 的 generation/abort/secondary 并发 2 语义；custom since/until 窗口回退 full 路径保契约语义）：

1. interactive 响应到达 → 计算客户端语义指纹 `dashboardFingerprint`（稳定序列化，剔除 `overview.generated_at` 与 `sync_command_center.generated_at` 两个 `now_utc()` 字段，剔除 `_meta` 渲染态元数据）。
2. 与当前渲染态指纹一致 → **跳过 `renderPrimaryDashboard`**；secondary 仍按保守方案以并发 2 重取，各 section 到达时先算 `panelFingerprint`，一致则不调 `buildContext`、不写 DOM（先指纹后 context 的短路顺序）。
3. 不一致 → `renderPrimaryDashboard`，其内部 8 个面板各自 dirty-check（`renderPanel` + `panelFingerprintCache`），只有指纹变化的面板写 DOM。

 silent 模式（`{ silent: true }`，自动刷新用）不设置 `secondary_refreshing`、不预渲染 stale 提示，避免 30s 周期性的 notice 闪烁；sync 完成用非 silent（数据大概率已变，保留 stale 语义）。契约红线未动：请求/响应字段零改动、secondary 并发 2、stale/loading 元数据保留至本代全部 settle、generation/abort 语义原样（快速连切 1d→7d→30d→all 仍由 generation + AbortController 防陈旧覆盖）。

已知一次性过渡：首屏 full snapshot 的 `health` 含完整 cursors 数组，interactive 响应为 `{ cursor_count }` 摘要——首次自动刷新 tick 时 hero/hero 相关面板指纹必变，会重渲一次；数据语义等价，之后 interactive→interactive 稳定命中。

辅助幂等写保护（同值不写，消除 MutationObserver 噪音）：`syncRangePresetControls` / `syncFilterControls` / `syncAutoRefreshControls` 的 `aria-pressed`；`syncPanelToggleControls` 的 `aria-expanded` 与 toggle 文案 `textContent`；model datalist 以 `dataset.optionsKey` 记忆；`updateSyncButton` 的 `innerHTML`（模块级 `lastSyncButtonHtml`）、`dataset.jobStatus` 与 `endpoint-sync` 文本。

## 4. 面板渲染路由（A2.1 / A2.4）

- 展开/折叠（app.js `setupPanelToggles`）：`renderDashboard` 全量 → `renderExpandedPanel(panel)` 只重渲对应面板（models/projects/costs，expanded 进入该面板指纹 extra），无关面板零写入。决策：未采用"纯 class 切换"——展开改变的是可见行数（slice 前 N → 全部），需要真实 DOM 行变化，单面板重渲已满足 R2.2。
- explorer 应用/重置（`reloadExplorer` 两处）：`renderDashboard` → `renderExplorerPanel`。
- locale 切换：`renderDashboard` 调用形式不变，但 `buildContext` memo 命中（零数据重算）+ locale 指纹失效（文案面板全部重渲）——即 PRD 修正后的目标。
- behavior 拆分：`renderBehavior` 单函数管 4 section → `renderActivity` / `renderTools` / `renderOptimize` / `renderCompare` 各写自己的子容器（`activity-*` / `tools-*` / `optimize-*` / `compare-panel`），`renderBehavior` 保留为全量路径组合。app.js secondary 到达回调 `renderSecondarySection` 按 section 路由，activity 到达只触碰 activity 容器（node 测试 "behavior sections render only their own containers" 以 DOM stub 断言 mutation 范围）。stale/`refreshNotice` 语义原样保留（refreshing 时 notice 前置拼接，settle 后指纹 extra 变化触发重渲消失）。
- dirty-check 位置决策：集中在 app.js `renderPanel` 注册表（单一 `panelFingerprintCache`），而非散在各 render 函数内——指纹模块（`data/fingerprint.js`）纯逻辑可独立进 node 测试，render 函数保持"只写 DOM"单一职责。

## 5. job 轮询（A2.6）

before：每 900ms 无条件 `updateSyncButton`（按钮 `innerHTML` 重建）+ `refreshSyncCommandCenter`（同步中心整块 `innerHTML` 重建）；click 逐节点绑定（每次重建后重新 `querySelectorAll().addEventListener`）。

after：

- `jobPollFingerprint`（stableSerialize 的浅字段摘要：status / job_id / summary / started_at / finished_at / last_event）比对，无变化 tick 零 DOM 写入；有变化才 `updateSyncButton` + `refreshSyncCommandCenter`（后者再经 syncCommandCenter 面板指纹 + job overlay extra 二次确认）。DOM 写入量与变化字段数成正比：0 变化 → 0 写入。
- 终态（completed/failed/cancelled）停止轮询（原有语义），新增总时长上限 `JOB_POLL_MAX_DURATION_MS = 30min` 防死循环。
- `sync-command-center.js` 的 click 绑定改容器级一次委托（`host.dataset.actionDelegateBound` 去重，host 元素不随 innerHTML 重建更换，监听器零重复绑定）。

## 7. fixture 冒烟与 benchmark 证据（A2.4 / A2.5 / A2.7）

fixture：`cargo run --features testing --example docs_dashboard_serve -- --port 37426 --timeout-secs 600`。

- `GET /` → 200；`GET /assets/app.js` → 200 且含全部新标记（`panelFingerprintCache` / `renderExpandedPanel` / `reloadDashboardAfterDataChange` / `jobPollFingerprint` / `data/fingerprint.js` 等）；`GET /assets/data/fingerprint.js` → 200（manifest 注册生效）；`GET /api/dashboard?scope=interactive&range=7d&window=week` → 200，响应键与契约一致（`generated_at` 维持在响应中，仅客户端指纹层剔除）。
- benchmark（`scripts/benchmark-dashboard-range.mjs --url http://127.0.0.1:37426 --iterations 5`，证据 JSON 见同目录 `benchmark-fixture-evidence.json`）：HTTP 部分 interactive p95 = 7.63ms（预算 ≤400ms）、max payload = 4351B（预算 ≤128KiB）；rapid 连切 1d→7d→30d→all：点击反馈 0.9ms（预算 ≤100ms）、最终选中 all（无陈旧覆盖）、4 个 interactive 请求、零 full scope 请求。
- 行为正确性（headless Chrome + MutationObserver 实测）：docs fixture 数据硬编码于 2026-04（`src/testing/mod.rs:216`），1d/7d/30d 窗口响应逐字节相同。实测点击序列：1d → trends-table 渲染（首渲染，18.2ms）；7d/30d → **数据相同，零 DOM 写入**（dirty-check 正确跳过，trends-table 内容不变——空表格语义相同）；all → 数据不同（多一行），正常渲染（12.4ms）。即"数据变→渲染、数据不变→跳过"在真实浏览器得到验证。
- 注意：原版 `benchmark-dashboard-range.mjs` 的 "critical render" 判定（trends-table 必出 mutation）在"相邻 range 数据逐字节相同"的陈旧 fixture 上与 dirty-check 语义冲突（7d 起等待超时）。这不是功能回归——A2.7 指定的测量对象是"代表性数据库只读副本"（各 range 数据不同，渲染必然发生），本环境无代表性数据库副本；fixture 上通过容错版脚本（catch 后继续）完成行为验证，原始脚本逻辑未改。

## 6. 复核记录

- rawData 全链路不可变替换式更新：grep `rawData` 赋值点（app.js 8 处）均为 `await load...` 或 `{...spread}` 替换，无原地 mutate；`stripVolatileFields` 亦不 mutate 入参（node 测试断言）。
- 易变字段核实：服务端 payload 中 `now_utc()` 仅 `overview.generated_at`（src/query/mod.rs overview 构造）与 `sync_command_center.generated_at`（同文件 sync command center 构造）；无服务端生成的相对时间文案（grep `分钟前|ago|relative` 无命中），相对时间均为渲染层从时间戳派生，不入指纹。
- 行尾：app.js / behavior.js / sync-command-center.js 为 CRLF+LF 混合，编辑按段保留原行尾（python 字节级校验无 lone CR，未整文件转换）；format.js / fingerprint.js / 测试文件为 LF。
