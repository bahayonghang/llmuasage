# TUI sync 动作嵌套 runtime 崩溃修复与后台任务化

父任务：`.trellis/tasks/07-20-tui-optimization`。证据：父任务 `research/llmusage-tui-analysis.md` §1/§5/§6、`research/tokscale-patterns.md` §1。

## Goal

修复 TUI 内按 `x` 触发 sync 的嵌套 runtime 崩溃风险，并把 sync 改为后台任务：sync 运行期间 UI 持续渲染与响应，展示进度，完成后自动刷新数据。

## Confirmed Facts

- 调用链：main.rs:1 `#[tokio::main]`（多线程 runtime）→ lib.rs dispatch async → dash.rs:31 `run()` async → 同步调用 `tui::run_terminal`，即整个 TUI 事件循环运行在 tokio worker 线程上。
- src/tui/mod.rs:179-207 `run_sync_action`：`tokio::runtime::Builder::new_current_thread().build()` 后 `block_on(sync::run_store_once_with_options)`。在 tokio worker 线程上 block_on 会触发 tokio 嵌套防护 panic（"Cannot start a runtime from within a runtime"）。该路径无任何测试覆盖。
- 即使规避 panic，block_on 全量导入也会冻结渲染线程数十秒（冷跑基线约 30s，见 `.trellis/tasks/archive` 中 07-20-sync-full-profiling 证据）；期间 Tick 在无界 mpsc 中积压（src/tui/event.rs:19）。
- 仓库已有完整后台任务系统 `sync/job_registry.rs`：`JobRegistry` 单任务准入、`tokio::spawn` 运行、`SyncEvent` 进度经容量 128 的 `tokio::sync::mpsc` 流出、CancellationToken 取消、可轮询 JobSnapshot。web 层已在用。
- 准入边界事实（2026-07-20 评审补充）：JobRegistry 是进程内内存态（docs/adr/0005-job-registry-in-memory.md），web 在 `WebState::new` 自建实例（src/web/mod.rs:57-59），且 `dash` 与 `serve` 本就是不同进程——registry 不能也不需要跨进程协调；跨进程并发 sync 由 worker lock 单写者契约裁决（source-sync-contracts.md §3）。
- `JobState` 只持有 snapshot + CancellationToken，不持有 JoinHandle（src/sync/job_registry.rs:68-71）——「退出后交由 registry 生命周期管理」不成立：进程退出即任务消亡，中断安全性依赖 sync_writer 的分片事务原子性。
- 对标 tokscale：后台线程加载 + 主循环 `try_recv` 排水 + footer 进度指示（`research/tokscale-patterns.md` §1、§6.2）；tokscale 的 DataLoader 还专门做了嵌套 runtime 防护（data/mod.rs:349-361）。
- sync 完成后现有行为：`invalidate_cached_data` + 重载当前面板 + 状态行显示 inserted/stored 摘要（src/tui/mod.rs:196-203）。

## Requirements

- R1 崩溃修复：`x` 键路径不得在渲染线程上 block_on/新建 runtime。TUI 已运行在多线程 tokio runtime 内，应通过 `tokio::runtime::Handle` 提交后台任务（优先复用 `JobRegistry`）。准入分两层：进程内重复触发由 registry admission 拒绝；跨进程并发（如 `serve` 正在跑 sync）由既有 worker lock 裁决，TUI 须把「锁被其他进程持有」作为一等失败态优雅呈现（状态行提示，不崩不挂）。
- R2 非阻塞：sync 运行期间事件循环照常 draw/收键；再次按 `x` 不重复启动（提示已在运行）。`q` 退出语义必须显式实现（registry 不管理任务存续，见 Confirmed Facts）：默认方案为 cancel + 有界等待终止（TUI 层持有 JoinHandle 或轮询 snapshot 终态，超时上限 design 阶段定，须尊重取消边界的分片原子性）；若选择「直接退出任由任务随进程消亡」，须在 design 中论证 writer 原子性下的安全性并记录。
- R3 进度展示：状态行（footer）展示 sync 进度（阶段/来源/事件计数，取自 SyncEvent 流或 JobSnapshot 轮询，250ms tick 排水即可）；完成后展示现有摘要文案并触发数据失效+当前面板重载。
- R4 取消：提供取消途径（如 sync 运行中再按 `x` 或 `Esc` 语义，design 阶段定），走 CancellationToken，尊重 source-sync-contracts.md §3 的取消边界语义。
- R5 契约红线：不改 sync 语义与 SyncRunOptions 行为；`source-sync-contracts.md` 的 sync 摘要/状态载荷契约保持不变。
- R6 测试：新增覆盖——(a) 在 `#[tokio::test]`（多线程 flavor）内驱动 sync 动作路径，证明不 panic；(b) 进度事件到状态行文案的纯逻辑测试；(c) 重复触发/取消的状态机测试。

## Acceptance Criteria

- [ ] A1：`llmusage dash` 按 `x`：不 panic；sync 期间界面持续重绘、可切面板；footer 有进度；完成后数据自动刷新且摘要文案与现状一致。
- [ ] A2：sync 运行中重复 `x` 不产生并发任务（单任务准入证据：日志或测试断言）。
- [ ] A3：R6 三类测试通过；`cargo test -- --test-threads=1`、fmt、严格 clippy 全绿。
- [ ] A4：`q` 退出与取消行为符合 design 中的定义，且终端正常恢复（panic hook 路径不回归）。

## Out Of Scope

- sync 本身的吞吐优化（07-20-sync-cold-import-write-throughput）。
- 面板查询异步化（07-20-tui-async-panel-loading）。
- web 层 sync 行为改动。

## Notes

- 复杂任务：`task.py start` 前需补 `design.md`（JobRegistry 接入方式、退出/取消语义、进度采样）与 `implement.md`。
