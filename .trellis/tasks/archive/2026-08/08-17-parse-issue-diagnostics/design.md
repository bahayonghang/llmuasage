# Design: parse issue 诊断与 ZCode skip 去重

## Domain

`ParseIssueSample` 增加 `reason: String`，`#[serde(default)]`。
`ParseIssues::record` 增加 `reason: &str`；空串表示「只有 kind/offset」。
`reason` 写入前截断到 `MAX_PARSE_ISSUE_REASON_CHARS`（64），只保留 `[A-Za-z0-9_:-]`。
四类 kind、`total()`、`informational_total()`、样本上限 8 不变。

ZCode 未完成行的 reason：`zcode_unfinished:{status}:{error_type}`。
`status` 只映射 `error` / `cancelled`，其余为 `other`。
`error_type` 来自 `model_usage.error_type`；列缺失或未通过字符集则 `unknown`。
不读 `error_message`。

## Skip watermark

`ZcodeCursor` 增加与 completed 水位平行的一对字段：

- `last_skipped_at: i64`
- `last_skipped_ids: Vec<String>`

Schema v22：`source_cursor.last_skipped_at INTEGER NOT NULL DEFAULT 0`、
`source_cursor.last_skipped_ids_json TEXT`。
只给 ZCode 读写。不复用 `last_processed_ids_json` 或 `last_total_json`。

`count_skipped_rows` 改为按行选择 `id, status, error_type?, completed_at`，条件：

```
status != 'completed'
AND (completed_at > completed_wm OR (completed_at = completed_wm AND id > completed_id))
AND (completed_at > skip_wm OR (completed_at = skip_wm AND id 不在 last_skipped_ids))
AND (recent_cutoff 为空或 completed_at >= cutoff)
```

每条新行记一次 `Skipped`，`offset = completed_at.max(0)`，带 reason。
全量且未取消时，用本批最大 `completed_at` 与同毫秒 id 集合推进 skip 水位。
`--recent-days` 与取消不写 skip 水位。
completed 锚点缺失时两个水位一起清零。

JSONL 源不设 skip 水位；未变化文件不会重解析，自然不重复计数。

## Data flow

```
ZCode SQLite
  → count_skipped_rows（相对 completed + skip 水位）
  → ParseIssues（计数 + 最多 8 条带 reason 的样本）
  → SourceSyncStats
  → driver 一条 info 事件
  → source_sync_status.parse_issues_json
  → sync 摘要 / source-status
```

看板与 TUI 继续只读四类计数。

## CLI

`parse_issue_sample_line`：

- 始终打印 kind
- `reason` 非空则打印 reason
- `@offset` 只在 `offset > 0` 且 `reason` 为空时打印（JSONL）
- basename 仍只在 file cursor 能解析时打印
- 禁止打印 `path_hash` 与原文

因此 ZCode 首次报告是 `skipped zcode_unfinished:error:invalid_request`，不再是 `skipped @0`。

`source-status` 在 `summary_text` 下复用同一格式打印样本行。

## Runtime log

`parsers/driver.rs` 在每个 source `parse` 返回后，若 `summary_text()` 有值，发一条 `info!`：
`source`、四类计数、`reasons`（样本 reason 的逗号拼接）。
不在 parser 内再打 per-row debug 作为主诊断。

默认 `LLMUSAGE_LOG=warn` 不落盘这条 info。这是有意的：skipped 不是故障。
单测用 tracing subscriber 捕获事件，不依赖默认文件级别。

## Compatibility

- 旧 `parse_issues_json` 无 `reason` → 空串。
- 旧 ZCode cursor 无 skip 列 → 迁移默认 0 / NULL，首次升级后只报告 completed 水位之后尚未记过的未完成行（本机即那条 `invalid_request`），随后静音。
- 新二进制写带 `reason` 的 JSON；旧二进制读未知字段应忽略（serde 默认）。不要新增 kind。
- 迁移走现有 fenced bootstrap / `ensure_column`。

## Trade-offs

- skip 水位与 completed 水位分开，避免把失败行 id 写进 completed 锚点，晚完成的 completed 行仍能导入。
- 默认可诊断靠 CLI + SQLite status，而不是降低文件日志级别。避免把每次 sync 的 info 噪音写进 10 MiB 分片。
- 重建 DB 会再报告一次未完成行。这是锚点丢失的既有语义，不做终身 skip 黑名单。

## Rollback

回退代码即可。v22 列可留着；旧代码不读它们。
带 `reason` 的 `parse_issues_json` 对只认识旧字段的读者仍然可读。
