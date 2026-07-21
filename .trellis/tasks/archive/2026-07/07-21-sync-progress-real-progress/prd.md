# sync 进度条真实进度显示

## Goal

让 `llmusage sync` 在 Claude/Codex 文件解析期间持续报告真实 replay 进度，消除确定进度条长期停在 `0/N`、结束时才瞬间跳满的现象，同时保持现有提交原子性、输出通道与性能边界。

## Background And Confirmed Facts

- 当前基线为 `dev@a9bf4ac`。TTY 渲染层位于 `src/commands/sync_progress.rs:264-324`，Claude/Codex 使用确定进度条，position 由 `SyncEvent::Progress.files_scanned` 驱动。
- Claude 在 `src/parsers/claude.rs:180-227` 按有界 project batch 启动 `spawn_blocking`；整个 batch 解析并合并提交后才发一次 `Progress`。单个大 project/batch 解析期间没有事件。
- Codex 在 `src/parsers/codex.rs:152-193` 按 shard 完成和提交后发事件；单个大 shard 内同样没有事件。
- 当前 `SourceStarted.files_total` 是 inventory 全量，而 `Progress.files_scanned` 是 replay/commit 文件数，分母与分子不一致。`BarRenderer::complete_active` 在 `SourceFinished` 时强制补满，只是终端清理兜底，不能证明生产端自然完成。
- Claude 的实际 replay 工作集不是 trigger 文件数：依据 `.trellis/spec/llmusage/backend/source-sync-contracts.md:36-40`，同一 project 任一文件变化时，该 project 当前全部 JSONL 都会 replay。Claude 的准确工作量是所有选中 project 的 plan 文件数之和；Codex 的准确工作量是 cursor 判定后进入 plans 的文件数。
- `SyncShard`/`commit_shard` 的 reset -> event -> cursor 原子协议受 ADR 0002 约束。Claude 将同一有界 batch 合并为一次事务是前序性能修复的刻意设计，不能为进度展示拆成 per-file commit。
- 进度 sink 经 driver 的容量 128 mpsc `try_send` 非阻塞投递；通道满时允许丢弃并记录 `progress_dropped`。人类进度写 stderr，`sync --json-events` 写 stdout NDJSON。
- `SourceFinished` 后清除活动 bar 并输出永久完成行是既有设计，本任务不改变该生命周期。

## Decisions

- D1：Claude/Codex 的确定进度条采用本轮实际 replay 文件数。Claude 统计所有选中 project plans 的文件总数，Codex 统计 cursor 判定后进入 plans 的文件总数；不以 inventory 全量为 denominator，也不把 skipped 文件计入 `files_scanned`。
- D2：解析进度事件最大采样频率为 5Hz（200ms）；仅在 completed counter 增长时周期 emit。进入 commit 前强制发送当前解析快照，commit 后再发送包含最新 `records_imported` 的快照，这两类边界事件不受采样频率限制。
- D3：TTY 确定进度条到达 `N/N`、但 source commit 尚未完成时，message 显示“重放完成，正在提交...”；由 BarRenderer 根据 position/length 推导，不扩展事件结构。LineRenderer、NDJSON 与 TUI 保持结构化事件原义。

## Requirements

### R1. 统一工作单位

- Claude/Codex 的确定进度条 length 与 position 必须使用同一工作单位。
- 工作单位为“本轮实际 replay 的文件数”：Claude = 选中 project plans 的文件总数；Codex = cursor 判定后的 planned files。
- `SourceSyncStats.files_processed/changed_files/skipped_files` 继续遵守 source sync spec，不因 UI 进度而改义。

### R2. 解析期进度采样

- Claude/Codex 的 `spawn_blocking` worker 每完成一个文件后更新共享的无锁计数器；async 侧按有界频率采样，仅在计数增长时 emit `Progress`。
- 周期采样最大 5Hz（200ms）；短任务在进入 commit 前强制发送当前解析快照，不为追求动画频率增加周期事件。
- 采样不得为每个文件执行 channel send、终端绘制或 SQLite 写入；worker 热路径最多增加一次 relaxed atomic increment。
- batch/shard commit 前必须 emit 当前解析快照，使短任务和最终 batch 能在写入期间显示真实 position；commit 后再 emit 包含最新 `records_imported` 的快照。成功结束前最后一个 Progress 必须自然到达 planned total。
- `records_imported` 继续表示已 commit 的累计记录数；解析计数与提交计数允许暂时不同。

### R3. 保持存储与取消语义

- Claude 保留 project-scoped replay、bounded `spawn_blocking`、同 batch 合并 `SyncShard` 与一次 `commit_shard`。
- Codex 保留 file-cursor incremental 与 shard commit 结构。
- 不降低 `commit_shard` 原子性，不引入 per-file commit，不扩大 cancellation 的既有保证。

### R4. 输出兼容

- 不新增 `SyncEvent` variant 或字段；serde/NDJSON wire shape 保持不变。
- `SourceStarted.files_total` 对 Claude/Codex 的语义从 inventory 总量纠正为 planned replay 文件数；OpenCode 的 spinner/row-count 路径不变。
- TTY、LineRenderer、`LLMUSAGE_PROGRESS` 强制 line、非 TTY、`sync --json-events` 与 TUI 均继续消费同一事件流。
- 文案不得把 replay 总量描述成 inventory 全量；最终 inventory/skip 统计继续由 `SourceFinished.stats` 表达。
- TTY position 到达 length 后、`SourceFinished` 前显示“重放完成，正在提交...”，避免长 commit 阶段表现为满条卡住；不为此新增事件或影响非 TTY wire 输出。

### R5. 性能边界

- 进度采样与发送保持在解析 worker 之外；沿用 `RenderStats` 与 driver `progress_dropped` 观察口径。
- 在同一 fixture/snapshot、相同构建与输出模式下做至少 3 次前后对照；sync wall-time 中位数回归不得超过 5%，并报告波动范围。缺少可比测量时不得标记通过。

## Acceptance Criteria

- [x] A1/R1：Claude/Codex 的 `SourceStarted.files_total` 等于本轮 planned replay 文件数，所有 `Progress.files_scanned <= files_total`，成功完成时最后一个 Progress 自然等于 files_total。
- [x] A2/R2：一个 batch/shard 内含多个、累计解析耗时超过一个采样周期的文件时，commit 前可观察到至少一个 `0 < files_scanned < files_total` 的中间事件；整轮可观察到不止一个 Progress 快照。
- [x] A3/R2：短于采样周期的运行仍在 commit/finish 前发出准确最终 Progress，不依赖 `BarRenderer::complete_active` 补满。
- [x] A4/R4：TTY 确定进度条到达 `N/N` 后显示“重放完成，正在提交...”，并在 `SourceFinished` 正常落永久完成行；LineRenderer/NDJSON 文案与 wire shape 不增加 commit-only 事件。
- [x] A5/R3：Claude project replay 与 batch 合并提交回归、Codex append 增量回归保持通过，stats 语义不变。
- [x] A6/R4：现有 `sync_progress.rs` 测试、LineRenderer/非 TTY/`LLMUSAGE_PROGRESS` 测试、`sync --json-events` NDJSON 测试与 TUI progress 测试通过；OpenCode 事件与 spinner 行为无变化。
- [x] A7/R5：前后性能对照满足中位数 <=5% 回归，并记录 `RenderStats.calls/nanos` 与 `progress_dropped`；9 次对照的 wall 中位数变化为 -4.41%。
- [x] A8：`cargo fmt --all -- --check`、严格 Clippy、相关 focused tests、完整串行 Rust tests 与 `git diff --check` 通过。

## Out Of Scope

- 改变 SourceFinished 后 bar 清理与永久完成行生命周期。
- OpenCode spinner、message/part row cursor 或进度单位改造。
- 新 CLI 参数、新依赖、schema 或持久化格式变更。
- 在单个超大 JSONL 文件内部按行/字节报告进度；本任务粒度为完成的文件。
- 借进度修复重写 parser、writer 或 source inventory 算法。
