# 完善 parse issue 诊断与日志

## Goal

用户第一次看见某条 ZCode 未完成调用时，能从 sync / source-status 读出「跳过了什么、为什么跳过」。同一条未完成行在后续 unchanged sync 上不再出现。分类四类不变；本任务补诊断与去重，不把失败行当用量导入。

## Background

2026-08-17 本机 `llmusage sync` 在 zcode 上打印 `skipped=1` / `skipped @0`，同时 `CHANGED=0`、`STORED=1610`。这不是解析失败，也不是丢了 completed 用量。

归档任务 `.trellis/tasks/archive/2026-08/08-17-parse-issue-observability/` 已把未完成行从 `malformed` 改成 `skipped`，并去掉 informational 类的警告色。样本仍只有 `kind + offset + 可选 basename`，对 SQLite 源不够用。

## Confirmed Facts

- 本机 `~/.zcode/cli/db/db.sqlite::model_usage`：completed=1610 已入库；error=15、cancelled=2 不入库。唯一越过 completed 水位的未完成行是 `status=error`、`error_type=invalid_request`、token 全 0、`completed_at` 比最新成功调用大约晚 1.4s。未读 `error_message`。
- ZCode 只导入 `status='completed'`。`error`/`cancelled` 由 `count_skipped_rows` 按 completed 水位计数，并对每条调用 `issues.record(..., offset=0, Skipped)`。`src/parsers/zcode.rs:339-367`
- 未完成行不推进 `ZcodeCursor.last_completed_at`，所以这条 `invalid_request` 会在之后每一次 unchanged sync 再出现。`src/store/cursor.rs:139-167`
- CLI 样本是 `kind @offset [basename]`；basename 只从 file cursor 解析。ZCode 的 `source_cursor.file_path` 为空，因此只剩 `skipped @0`。`src/commands/sync.rs:326-347`、`src/commands/sync_summary.rs:230-240`
- 样本契约禁止原文、prompt、完整路径；每源最多 8 条。`source-sync-contracts.md:335-341`
- 运行时 NDJSON 默认 `LLMUSAGE_LOG=warn`。现有 skip 日志是 `tracing::debug`，默认不落盘，`llmusage logs` 看不到。`src/runtime/logging.rs:22`、`src/runtime/logging.rs:454-459`
- doctor 只在 `total()`（malformed+oversized）>0 时 warn；看板/TUI 只有四类计数、无样本。这两点保持。

## Requirements

### R1 样本可解释

- `ParseIssueSample` 增加 `reason`（serde default 空串）。闭集短码，例如 `zcode_unfinished:error:invalid_request`。禁止 `error_message`、prompt、原文、完整路径、完整 row id。
- `reason` 的 `status` / `error_type` 只允许 `[A-Za-z0-9_-]`，最长 64；缺列或非法值用 `unknown`。测试夹具无 `error_type` 列时也必须安全降级。
- SQLite 源不得再把恒 0 当作唯一 locator。人读在有 `reason` 时不打印 `@0`。
- JSONL 源继续用 byte offset；有 basename 时仍打印 basename。
- 样本预算仍 ≤ 8；计数饱和累加。

### R2 CLI / source-status

- sync 人读样本必须能看出 kind + reason。
- `source-status` 对有样本的来源打印同样的 reason 行（仍不打印原文）。
- 警告色不变：只有 malformed/oversized 用黄色。
- 看板 / TUI / `SyncSourcePayload` 仍只带四类计数，不带样本或 reason。

### R3 运行时日志

- 每个来源在本 run 出现非零 parse issue 时，由 driver 写 **一条** info 级结构化事件：source、四类计数、样本 reason 列表。不写行正文。
- 默认文件级别保持 `warn`。info 事件只在 `LLMUSAGE_LOG=info`（或更细）时落盘；`llmusage logs --level info` 在该配置下能读到。
- 默认 `llmusage logs`（warn 窗口）不把 skipped-only 当错误刷屏。
- 默认可诊断通道是 CLI 摘要 + 持久化的 `parse_issues_json`，不是默认 NDJSON。

### R4 同一条未完成行只报告一次

- 用户决定：后续 unchanged sync **不要再出现**同一条 ZCode 未完成行。
- ZCode 单独持久化 skip 水位（`last_skipped_at` + `last_skipped_ids`），与 completed 水位分开。
- 只把「相对 skip 水位为新」的未完成行计入本 run 的 `skipped_lines` / 样本 / info 事件。
- 第二次 unchanged sync：`skipped_lines=0`、无样本、无新 parse-issue info 事件。新出现的未完成行仍报告一次。
- `--recent-days` 与取消不得推进 skip 水位（对齐 completed 水位）。
- completed 锚点缺失 / DB 重建时，skip 水位与 completed 水位一起重置；重建后的未完成行允许再报告一次。
- 不得把 `error`/`cancelled` 行导入为 `UsageEvent`。
- 去重只针对 ZCode 这类 SQLite 高水位计数。JSONL 源仍靠 file cursor，不另做 skip 水位。

### R5 契约与兼容

- 更新 `source-sync-contracts.md` 与 `runtime-log-contracts.md`。
- skip 水位用 schema 迁移加列，走现有 fenced bootstrap，不复用 `last_total_json` / `last_processed_ids_json`。
- 不改 token 公式、4 MiB 上限、四类互斥、doctor 故障定义。
- 旧 `parse_issues_json` 缺 `reason` 时按空串读。

## Acceptance Criteria

- [ ] AC1 夹具复现「completed 水位后 1 条 `invalid_request`」：第一次 sync 的 skipped 样本含 reason，不含 `@0`，不含 row 正文。
- [ ] AC2 completed 行仍全部入库；error/cancelled 仍不入库。
- [ ] AC3 `parse_issues_json` 样本含 reason；旧 JSON 缺字段仍能读。
- [ ] AC4 driver 在非零 parse issue 时发一条 source-scoped info 事件（单测可捕获）。默认 warn 文件不要求落盘该事件；`LLMUSAGE_LOG=info` 时 `llmusage logs --level info` 能看到 reason。
- [ ] AC5 JSONL 源（Codex skipped/oversized）样本仍带 byte offset，不回归。
- [ ] AC6 doctor 对 skipped-only 仍为 ok。
- [ ] AC7 更新上述两份 spec；既有 zcode / bounded JSONL / sync-summary 测试不回归。
- [ ] AC8 同一夹具第二次 unchanged sync：`skipped_lines=0`、无样本、无新 parse-issue info 事件。再插入一条更新的未完成行后，只报告这一条。
- [ ] AC9 `--recent-days` 不推进 skip 水位；ZCode DB 重建后 skip 水位重置，未完成行可再报告一次。

## Out of Scope

- 不导入 ZCode `error`/`cancelled` 用量。
- 不把默认文件日志改成 info/debug/trace。
- 不记录 `error_message` / `raw_usage_json`。
- 不做跨 run 的历史 skipped 累计看板。
- 不重开 4 MiB 上限或残缺 JSON 修复。
- 不为 OpenCode 新造 parse issue。
- 不把样本或 reason 打进交互 dashboard / TUI payload。

## Key Decisions

- 同一条未完成行只在首次看见时出现；之后靠 skip 水位静音。
- 默认可诊断面是 CLI + `parse_issues_json`；runtime info 日志是可选落盘。
- `reason` 只用闭集短码，不升级默认日志级别，不把 skipped 标成故障。
