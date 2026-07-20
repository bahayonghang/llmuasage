# sync 摘要展示优化

父任务：`.trellis/tasks/07-20-sync-progress-and-perf`（R3 归属本任务；依赖 console 由本任务声明）

## Goal

把 `Sync finished` 统计块（src/commands/sync.rs:223-244 `print_summary`）优化为对齐表格：补充 `bytes_scanned`（人类可读）、`parse_ms`/`write_ms` 耗时列，TTY 下着色，非 TTY 纯文本；格式化逻辑收敛为可单测纯函数。

## Confirmed Facts

- `SyncSummary { sources, total_seen, total_inserted, stored_events }`（sync.rs:21-27），未 Serialize，无 wire 契约负担。
- `SourceSyncStats`（parsers/mod.rs:202-234）字段齐备：files_processed/changed/skipped、bytes_scanned、events_seen/inserted、stored_events、parse_ms/write_ms、absent、last_error。
- 摘要写 stdout；现无针对 `print_summary` 文本块的测试。
- `console` 已在 indicatif 依赖树中（lifecycle 子任务引入 indicatif），本任务将其声明为直接依赖，版本对齐，无新增传递依赖（父 D5）。

## Requirements

- R1 纯函数 `format_summary_lines(summary: &SyncSummary, color: bool) -> Vec<String>`：`Sync finished:` 首行、rebuild 提示行（如有）、对齐表格、totals 行（seen/inserted_delta/stored_events）。
- R2 表格列：SOURCE FILES CHANGED SKIPPED SEEN COMMITTED STORED BYTES PARSE WRITE；数字右对齐，列宽按内容最大值对齐。
- R3 `bytes_scanned` 人类可读（自写格式化器，如 `895.3 MB`，不加依赖）；`parse_ms`/`write_ms` 渲染为 `350ms`/`1.2s`。
- R4 `absent` 来源数字列以 `-` 占位；`last_error` 存在时该来源下方附一行错误提示。
- R5 颜色经 `console::Style`，仅当 `stdout().is_terminal()`：来源名加粗、`committed>0` 绿色、`changed>0` 黄色；非 TTY 无任何 ANSI。
- R6 兼容：stdout 仍是唯一摘要出口；`--json-events` 路径不经过本函数；不改 `SourceSyncStats` 任何字段。

## Acceptance Criteria

- [ ] A1/R1-R4：单测覆盖：多来源对齐（含长短来源名）、color=false 无 ANSI、absent 占位、last_error 行、零值/大数字节与耗时格式化。
- [ ] A2/R5：TTY 手动查看一次着色效果；管道运行（PowerShell `cargo run -- sync | Out-String`）输出无 `ESC`。
- [ ] A3/R6：`cargo test -- --test-threads=1` 通过；`tests/sync_regression.rs:1236` wire 契约测试不受影响。
- [ ] A4：`cargo fmt --all -- --check`、严格 Clippy 通过。

## Out Of Scope

- 进度渲染（lifecycle 子任务）；新增统计字段或 stats 结构变更；`SyncSummary` Serialize。
