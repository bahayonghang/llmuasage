# serve 看板渲染生命周期与自动刷新优化（S2，P1）

父任务：`.trellis/tasks/07-22-serve-dashboard-ui-perf`（全局约束 H1–H5 继承自父 PRD）。技术方案见同目录 `design.md`。

## Goal

让"数据没变就不动 DOM、状态变了只动该动的面板"，自动刷新与 sync 完成后的重载走轻量 interactive 路径，消除一次用户动作触发 ~10 次派生计算与全量 innerHTML 重建的放大链。

## Requirements

- R2.1：面板级 dirty-check。每个渲染入口先比对"本面板数据指纹"，未变则跳过 DOM 写入；指纹为面板对应数据子集的序列化摘要，剔除易变字段（见 design.md）。(Fact C26)
- R2.2：renderDashboard 调用路由。面板展开/折叠（app.js:1192-1194）只重渲对应面板；explorer 应用（app.js:1130）只调 renderExplorer。locale 切换（app.js:113-117）的目标**不是**不重渲——locale 影响全部文案——而是复用 R2.3 的派生 context 缓存、零数据重算地重渲文案容器。(Fact C27，按评审修正)
- R2.3：`buildContext(rawData)` 按 rawData 引用缓存，一次数据到达只构建一次；`Intl.NumberFormat` 模块级缓存复用（data/format.js:11/31/43/63）。(Facts C28/C29)
- R2.4：renderBehavior 拆分为 section 级渲染函数（renderActivity/renderTools/renderOptimize/renderCompare；behavior.js:241-269 目前已按子容器写 DOM，但单一函数管全部 4 section）。每个 secondary 到达只更新自己的 section，保持契约"secondary 只更新自己 section + stale/loading 元数据保留至本代全部 settle"。**禁止**"收集齐统一渲染"方向。(Fact C30，按评审修正)
- R2.5：自动刷新（app.js:962-973）与 sync 完成后的重载（app.js:1473-1475）改走 `scope=interactive` + 按需 secondary（复用既有 fast-range 路径）；响应到客户端后计算**客户端语义指纹**（剔除 `overview.generated_at` 等每查询必变字段后的数据子集摘要），与当前渲染态指纹一致则跳过全部重渲染。明确：不新增/删除任何 API 契约字段（`generated_at` 维持现状，不作为指纹依据——它是 `now_utc()`，src/query/mod.rs:829/2256）。(Facts C31/C45，按评审修正)
- R2.6：job 轮询瘦身（app.js:1433-1443）：轮询快照比对后只更新变化的文本/DOM 节点（不再每 900ms 整块重建同步中心）；加最大轮询时长与退出条件（job 终态后停止）；sync-command-center 事件绑定改容器级委托（sync-command-center.js:372-376）。(Facts C32/C36)

## Acceptance Criteria

- [x] A2.1：面板展开/折叠时，无关面板 DOM 零写入（MutationObserver 计数或测试断言）；数据未变的自动刷新周期内，面板 DOM 零写入。
- [x] A2.2：locale 切换不触发 `buildContext` 重算（插桩计数断言），且文案全部正确切换（node 测试）。
- [x] A2.3：单次 range 切换中 `buildContext` 调用次数从 ~7 降为 1（插桩计数）；`Intl.NumberFormat` 构造次数为有界常数。
- [x] A2.4：fast-range 切换时，activity 到达只触碰 activity 容器（MutationObserver 范围断言）；快速连切 1d→7d→30d→all 无陈旧覆盖（既有 generation 语义保持，回归测试）。
- [x] A2.5：开启 30s 自动刷新且数据无变化时，观察期内客户端零面板重渲染、服务端无 full scope 请求（仅 interactive，日志/计数证明）；sync 完成后重载同样走 interactive。
- [x] A2.6：job 轮询期间每次 tick 的 DOM 写入量与变化字段数成正比（抽查）；job 到达终态后轮询停止。
- [x] A2.7：`node --check`/`node --test` 通过；`scripts/benchmark-dashboard-range.mjs` 对代表性数据库只读副本测得 interactive 预算达标（p95 ≤400ms、JSON ≤128KiB）；`just ci` 通过。

## Notes

- 复杂任务：启动前需本 PRD + design.md + implement.md（Phase 1 补齐 implement.md）齐备。
- 改动面：`src/web/assets/app.js`、`render/*.js`、`data/derive.js`、`data/format.js`、`data/fetch.js`（如需）。不动 Rust 侧。

## 验收记录（2026-07-22）

插桩与 MutationObserver 结果见 `research/render-lifecycle-instrumentation.md`；最终代表库 benchmark 见 S3 `research/after-benchmark-final.json`，interactive HTTP p95 为 13.80–46.28ms、最大 payload 55,801 bytes。dashboard Node 测试 16/16 与 `just ci` 均通过。
