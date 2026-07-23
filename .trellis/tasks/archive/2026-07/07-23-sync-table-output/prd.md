# 表格化 sync 成功终态输出

> 父任务：`07-23-multi-source-sync-table`。本子任务只做**展示层**，不新增来源、不碰 parser。

## Goal

让 `llmusage sync` 的成功终态在 stdout 上只输出**一张**逐来源 + `TOTAL` 行的可扫描表格，移除由 `SourceFinished` 造成的重复"来源完成句"，并保证该表在 TTY、非 TTY、重定向和窄终端下都可读、可测试、无控制序列污染。

## Background And Evidence

- 重复输出根因见父任务 `research/current-architecture.md`：`src/commands/sync_progress.rs:253-263`（TTY bar renderer）与 `:532-538`（line renderer / `human_progress_line`）都把 `SourceFinished` 变成永久完成句，`src/commands/sync.rs:283-287` 又在其后打印最终摘要，二者并行。
- 表格渲染器已存在且为纯函数：`src/commands/sync_summary.rs` 的 `format_summary_lines`（先按纯文本算宽度、对齐后再着色）。当前它输出一个独立的 `- totals:` 行，而不是表格行。
- 终端宽度自适应约定已存在于 `src/tui/report_table.rs`（`fit_widths`、`terminal_width` 读 `COLUMNS`、`detected_terminal_width`），但这些函数目前是 `report_table.rs` 私有，且该模块用 `crossterm` styling，而 `sync_summary.rs` 用 `console`。复用需先决定"抽公共 helper 还是重写"。

## Requirements

- R1. 成功终态只保留一张表：逐来源行 + `TOTAL` 行，保留现有 files/changed/skipped/seen/committed/stored/bytes/parse/write 列语义。`SourceFinished` 继续驱动实时进度，但不再输出永久完成句。
- R2. `Failed` 与 `Cancelled` 的诊断输出保留（它们是诊断，不是重复的成功摘要）。
- R3. 终态表在 TTY、非 TTY、重定向、窄终端下可读且可测试：stdout 只放最终表格，进度留在 stderr；非 TTY 不输出 ANSI；长来源名有稳定截断/紧凑降级策略，且只截断展示标签、绝不截断数值。
- R4. 保持 `SyncEvent` 与 `SourceSyncStats` 的 wire 形状不变；JSON/event 输出不变。`TOTAL` 行由现有 source stats 聚合得到，不新增字段。
- R5. 现有测试 `sync_summary.rs::totals_line_keeps_legacy_shape` 会因移除/折叠 `- totals:` 行而失效——本任务须显式更新它到新的 `TOTAL` 行契约，并说明这是预期变更而非回归。

## Acceptance Criteria

- [ ] AC1. 成功 `sync` 的 stdout 只出现一张逐来源 + `TOTAL` 表；stderr 的进度在终态前正确清理，不再重复打印来源完成句。（父任务 AC4）
- [ ] AC2. 表格 formatter 的纯函数测试覆盖 ANSI/非 ANSI、无来源、缺失来源、长名称、窄宽、`TOTAL` 聚合；非 TTY 输出无控制序列且列不发生不可理解错位。（父任务 AC5）
- [ ] AC3. `cargo fmt`、`cargo test sync_summary`、`cargo test sync_progress`、CLI stdout/stderr 分离子进程测试（含 `COLUMNS=60`/`COLUMNS=120` 等价）全部通过。

## Out Of Scope

- 不新增任何 `SourceKind` / descriptor / parser；不读取任何来源目录。
- 不改 SQLite schema、不改 query/report DTO。
- Kimi/Pi 的逐来源行会在各自子任务落地后自动出现在同一张表里，本任务只保证表结构能容纳任意 parser-backed 来源。

## Dependency And Coordination

- **可独立交付**：本任务对现有 3 个 parser-backed 来源（codex/claude/opencode）即可完整验收，不依赖 Kimi/Pi 子任务。
- **共享文件协调**：本任务会改 `src/commands/sync_summary.rs`、`src/commands/sync_progress.rs`，其中 `source_label(SourceKind)` 是非穷尽 match，Kimi/Pi 子任务会各加一个 match 臂。见 `implement.md` 的协调说明与"从 descriptor 取显示名以解耦"的可选项。
