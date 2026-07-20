# TUI 面板数据异步加载与并行查询基座

父任务：`.trellis/tasks/07-20-tui-optimization`。证据：父任务 `research/llmusage-tui-analysis.md` §1/§2/§5/§6、`research/tokscale-patterns.md` §1/§6。本任务是整个 TUI 优化的核心基座。

## Goal

把所有面板数据加载从渲染线程移到后台：面板切换/刷新/改筛选立即渲染 loading 态，数据就绪后经通道回填；重载荷面板的多个子查询并行执行；过期结果按世代丢弃。UI 线程从此只做渲染与输入。

## Confirmed Facts

- src/tui/mod.rs:248-323 `load_panel_data` 在按键处理内同步执行 Dashboard 查询，下一帧才绘制 → "Loading…/加载中…" 占位分支不可达，首访重面板 UI 冻结。
- 单个 `Dashboard`（一条 rusqlite Connection）服务全会话（mod.rs:85）；连接不可跨线程共享，但 store/connection.rs:21-36 开新连接成本低（WAL），web 层已按查询开新连接。
- 重载荷面板串行子查询：Stats=5（overview/heatmap 365 天/source_breakdown/health/context_pressure raw-event GROUP BY :1000）；Behavior=5（activity/tools 多 CTE :1305/optimize 4 检测器 :1442/zombie 文件系统扫描 :1514/model_compare ≈7 查询 :1778）；Blocks=全量 usage_event 流式扫描（reports.rs:763-810）。
- 契约事实（2026-07-20 评审补充）：source_breakdown 的逐 source `MAX(event_at)` 查询（query/mod.rs:1115-1127）是 dashboard-performance-contracts.md「Source totals」条款与 §7 Correct 示例明确要求的形态（走 `(source, event_at)` 索引，优于全表 GROUP BY）——不是缺陷，本任务不得改动该形态。
- 可复用模式：web/mod.rs:771-842 `load_via_dashboard_with_timeout` —— Semaphore 许可 → `spawn_blocking` 内开新 Dashboard → 发布 `InterruptHandle` → 超时可 `interrupt()`。契约：`.trellis/spec/llmusage/backend/dashboard-performance-contracts.md` §3（含 range-click 的「更新选中态于首个 await 前、世代+筛选双匹配才接受结果」语义）。
- tokscale 循环结构（可对标）：主循环阻塞于事件 `recv()`（Tick 100ms 兜底），后台结果 `try_recv` 排水，`background_loading` 标志驱动 loading/刷新指示（`research/tokscale-patterns.md` §1）。
- 事件通道现为无界 std mpsc（event.rs:19），主线程阻塞时 Tick 积压。

## Requirements

- R1 异步基座：新增 TUI 数据加载层——请求 = (panel, filter, time_window, generation)；执行 = `tokio::task::spawn_blocking` + 每查询新开 Dashboard 连接（复用或抽取 web 的许可/中断模式，Semaphore 上限沿用 WEB_DASHBOARD_QUERY_PERMITS 或单独常量）；结果经通道回事件循环（与现有 EventHandler mpsc 合流或 tick 排水，design 阶段定）。
- R2 世代语义：面板切换/筛选变更/时间窗变更递增 generation；仅接受 generation 与当前筛选双匹配的结果（对齐 dashboard-performance-contracts.md 的 range-click 契约）；被放弃的查询应尽力 interrupt。
- R3 loading 态可达：请求发出后立即置 loading 并渲染占位帧（现有 None 分支文案复用）；已有数据时刷新采用 stale-while-refresh：保留旧数据渲染 + footer/状态行「刷新中」指示（对标 tokscale background_loading）。
- R4 并行子查询：Stats 与 Behavior 的 5 个子载荷各自并行执行（各自 spawn_blocking + join；或拆为独立回填的子区块，design 阶段定）。source_breakdown 保持契约要求的 per-source last_event_at 查询形态不变；如后续测量证明其为瓶颈，须先另行修订契约（附 EXPLAIN QUERY PLAN 与代表性数据证据）再立任务，本任务不动。
- R5 行为保持：各面板展示数字与现状逐面板一致（同一数据库快照对比）；panel 数据缓存（Option 惰性加载、切走不重查）语义保留；auto-refresh/手动 r/来源筛选流程不回退。
- R6 退出安全：`q` 时在飞查询不得阻塞退出（interrupt + 有界等待或分离），终端恢复正常。
- R7 测试：(a) 世代丢弃——慢查询结果晚于新请求返回时不覆盖新状态；(b) loading 帧可达——TestBackend 断言首帧含占位文案；(c) 并行正确性——Stats/Behavior 载荷与串行结果一致；(d) 取消/退出不悬挂。

## Acceptance Criteria

- [ ] A1：首次进入 Behavior/Stats/Blocks：立即出现 loading 帧（TestBackend 证据），期间按键可切面板；数据就绪后正确填充。
- [ ] A2：快速连续切换面板/筛选：无过期结果覆盖（R7a 测试）；无查询悬挂残留（进程可干净退出）。
- [ ] A3：统一测量协议（代表性数据库快照 + release 构建 + 3 次取中位数，对齐父任务 X7 协议）下，Stats/Behavior 载荷 wall-time 较串行基线下降 ≥30%，且数字与串行一致；基线与结果记录到任务 research/。
- [ ] A4：R7 全部测试 + `cargo test -- --test-threads=1`、fmt、严格 clippy 全绿；X5 数据语义回归通过。
- [ ] A5：dashboard-performance-contracts.md 若因复用抽取而涉及 web 代码，web 行为零变化（现有契约测试全绿）。

## Out Of Scope

- sync 动作后台化（07-20-tui-sync-runtime-fix）。
- 查询本身的 SQL/范围优化（07-20-tui-time-window-bounding；本任务只并行化与搬线程，不改查询语义）。
- 渲染层 memo/脏标记（07-20-tui-render-efficiency）。

## Notes

- 复杂任务：`task.py start` 前必须补 `design.md`（通道拓扑、世代协议、连接/许可策略、与 EventHandler 合流方式、错误面）与 `implement.md`（分步接线顺序：先基座后逐面板迁移，保证每步可回退）。
