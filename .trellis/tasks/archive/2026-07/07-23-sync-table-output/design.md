# Design — 表格化 sync 成功终态输出

## Architecture And Boundaries

只改**展示契约**，不动来源/parser/存储：

- `src/commands/sync_progress.rs`：改 `SourceFinished` 的处理，使其关闭/刷新当前 bar 或 line，但不落永久成功句；保留 `Failed`/`Cancelled` 诊断与终端清理。
- `src/commands/sync_summary.rs`：`format_summary_lines` 保持纯函数与"对齐后着色"，新增一个由 source stats 聚合的 `TOTAL` 行，移除或折叠独立的 `- totals:` 行。
- 不改 `SyncEvent` / `SourceSyncStats` 结构；不改 `src/commands/sync.rs` 的事件流，只改它打印的最终文本来源（仍是 `format_summary_lines`）。

## Sync Terminal Contract

1. `SourceStarted` / `Progress` 继续在 stderr 渲染实时进度。
2. `SourceFinished` 关闭活动 bar/line，**不**输出永久成功句。`Failed` / `Cancelled` 保留诊断输出。
3. `format_summary_lines` 保持纯函数、先算宽度后着色；新增 `TOTAL` 行，字段来自各来源 stats 的聚合（files/changed/skipped/seen/committed/stored/bytes/parse/write）。旧的独立 `- totals: seen=… inserted_delta=… stored_events=…` 行移除或折叠进 `TOTAL` 行；JSON/event 输出不变。
4. stdout 只保留最终表格，进度留在 stderr，使重定向与子进程测试能断言"流纯净"。

## Width-Aware Rendering（本任务最高不确定性）

目标：窄终端下表格仍可读——选择完整或紧凑表头、只截断展示标签、绝不截断数值、非 TTY 不出 ANSI。

现状与接缝：

- `src/tui/report_table.rs` 已有 `fit_widths(columns, widths, terminal_width)`、`terminal_width()`（读 `COLUMNS`）、`detected_terminal_width()`，但均为该模块**私有**，且 `report_table.rs` 用 `crossterm::style`，`sync_summary.rs` 用 `console::Style`。
- **需在实现前定的决策（二选一）**：
  - (A) 把宽度/显示宽度 helper 抽到一个共享模块（如 `src/commands/` 或 `src/tui/format` 下的中立函数），`report_table.rs` 与 `sync_summary.rs` 共用；改动面更大但消除重复。
  - (B) 在 `sync_summary.rs` 内按同一约定重写一份轻量宽度探测 + 截断逻辑；改动面小但与 report_table 有受控重复。
- 推荐 (B) 起步（`sync_summary` 只有 10 列、语义简单，重写成本低），并在 design 记录：若后续第三处也需要同套逻辑，再回收为 (A)。

可选解耦（降低与来源子任务的 `source_label` 冲突）：

- 当前 `sync_summary.rs::source_label` 与 `sync_progress.rs::source_label` 都是对 `SourceKind` 的硬编码 match。可改为从 `registry::source_descriptor(kind)` 取显示名，这样新增来源不必回来改这两个展示文件。此项为**可选优化**，若采纳应在本任务落地，以便 Kimi/Pi 子任务"零展示层改动"。

## Compatibility And Rollback

- 若窄表适配过于侵入，回退策略：非 TTY 保留稳定的完整表，TTY 用一个有文档的紧凑表头模式；**绝不**回退成重复成功句。
- 不新增 migration；不改历史事件语义。

## Verification Shape

- `format_summary_lines` 纯函数单测：ANSI/非 ANSI、无来源、缺失（absent）来源、长名称、窄宽（`COLUMNS` 等价注入）、`TOTAL` 聚合正确性、着色不改变去色文本。
- CLI 子进程测试：分别捕获 stdout / stderr，断言 stdout 只含最终表、stderr 无重复完成句、非 TTY 无 ANSI。
- 更新 `totals_line_keeps_legacy_shape` 到新契约（见 `prd.md` R5）。
