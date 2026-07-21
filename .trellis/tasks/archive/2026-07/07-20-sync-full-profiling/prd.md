# sync 全链路 profiling

父任务：`.trellis/tasks/07-20-sync-progress-and-perf`（R4/R5 归属本任务）

## Goal

用可复现的测量协议对 `llmusage sync` 全链路（bootstrap/锁/parse/write/事件通道/渲染）做一轮 profiling，交付 `research/profiling.md`；测量确认的问题小则就地修复，大则在父任务下另建子任务。同时产出进度渲染开销的开/关对照数据（父 R5）。

## Confirmed Facts

- 既有计时：`SourceSyncStats.parse_ms/write_ms/lock_wait_ms`（parsers/mod.rs:202-234）。
- 事件通道 mpsc(128) + `try_send` 丢弃（driver.rs:80-83），丢弃无计数。
- 非 TTY 渲染每事件 `write!`+`flush()`（sync.rs:613-614）。
- 渲染开/关开关 `LLMUSAGE_PROGRESS=off` 由 lifecycle 子任务提供（本任务依赖其先完成）。
- 上一轮基线：`.trellis/tasks/archive/2026-07/07-19-claude-sync-scan-performance/prd.md`（Claude 899.6MB/约 106.5s 修复前；修复后数据见其 implement 记录）。

## Requirements

- R1 计时布点：bootstrap、锁获取、driver 逐来源、stored_events 查询（sync.rs:393-415）、摘要组装，走现有 tracing debug 输出；不加 CLI 旗标、不改 stats 结构。
- R2 通道观测：driver sink 增加 `try_send` 丢弃计数（原子计数器，结束时 tracing 输出）；不改丢弃策略本身。
- R3 测量协议（评审第 3 条修正）：
  - 固定夹具：真实规模源数据（Claude 多项目 + Codex + OpenCode fixture DB）放入 tempfile 隔离 home；
  - **每次运行前恢复快照**：源数据目录与目标 llmusage DB 都从快照复原，保证各运行面对同一增量状态；
  - 同输出目标对照：渲染开/关（`LLMUSAGE_PROGRESS=off`）在相同输出目标下各跑 3 次，报告中位数与波动范围（min-max）；
  - 开销结论以「同快照、同输出目标、渲染开 vs 关」为唯一有效对照（不比较 TTY vs 管道这种混入设备与状态差异的方案）。
- R4 必查候选（测量确认才算问题，未复现明确标注）：(a) `stored_events_for_source` 逐来源查询；(b) 通道 128 容量在提交尖峰的丢弃率；(c) 非 TTY flush 频率；(d) 锁心跳间隔；(e) 渲染线程事件处理速率。
- R5 修复纪律：确认的问题评估修复规模——小（单文件、无语义变更）就地修并附前后数据；大或涉语义则在父任务下另建子任务，本任务只留证据；任何修复对照 source-sync-contracts.md §3 不越界。
- R6 交付 `research/profiling.md`（本任务目录下）：环境（OS/CPU/构建 profile）、数据规模、各阶段耗时占比、通道丢弃计数、候选结论、修复前后对照、渲染开销对照。

## Acceptance Criteria

- [ ] A1/R1,R2：计时与丢弃计数布点合入，tracing debug 可观察到各阶段数据。
- [ ] A2/R3：profiling.md 中每次测量可复现：快照脚本/步骤、运行次数、中位数 + min-max 齐全。
- [ ] A3/R4：五个候选逐项有结论（确认并修复/确认并立项/未复现），证据随附。
- [ ] A4/父R5：渲染开/关对照数据齐全，结论明确（<2% 或解释）。
- [ ] A5：`cargo test -- --test-threads=1`、fmt、严格 Clippy 通过；就地修复附测试。

## Out Of Scope

- 无测量证据的优化；查询层/dashboard/TUI 性能；修改 OpenCode 源数据库。
