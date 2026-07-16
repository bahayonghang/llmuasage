# 第一次同步

`llmusage sync` 会把本地用量导入 SQLite。报表命令不会自动 sync；数据库过旧时需要先同步。

## 导入所有来源

```powershell
llmusage sync
```

人读进度写入 stderr，最终摘要保留在 stdout。

如果内置定价目录自上次运行后发生变化，bootstrap 会在扫描来源前重算历史事件价格。进度会显示新旧目录版本、已处理/总事件数、汇总桶对账和最终耗时。对未固定的内置目录，这是一轮一次性升级；目录已是最新或已固定时会跳过。

摘要会按来源显示 `files`、`changed`、`skipped`、`seen`、`committed` 和 `stored_events`。对文件型来源，`skipped` 表示已保存的 cursor、文件大小、mtime、头部 fingerprint、尾部签名和 offset 证明文件未变化。对 OpenCode，`skipped` 表示 SQLite 高水位 cursor 没找到更新行。`committed` 是 SQLite 去重后本次新增写入数；`stored_events` 是数据库中的持久总量。

## 只导入一个来源

```powershell
llmusage sync --source codex
llmusage sync --source claude
llmusage sync --source opencode
llmusage sync --source antigravity
# gemini 不再作为来源 id；gemini-* 模型名保持不变
```

合法来源与 `cargo run -- --help` 一致：`codex`、`claude`、`opencode`、`antigravity`。`gemini` 不再作为来源 id；`gemini-*` 仍只是模型名前缀。

其他平台可能在 `llmusage source-status` 或 `dash` 来源选择器中以仅监控候选出现。它们在具备脱敏 fixture、token 语义、sync-twice 测试、cursor/fingerprint 回归测试和隐私审查之前保持 parserless。

## 输出 NDJSON 进度

```powershell
llmusage sync --json-events
```

该模式会在 stdout 输出 NDJSON 生命周期/进度事件。定价升级会依次增加 `pricing_upgrade_started`、节流后的 `pricing_upgrade_progress`、`pricing_bucket_reconcile_started` 和 `pricing_upgrade_finished`，适合 wrapper 或 UI adapter 使用。

人读进度不依赖结构化日志。文件诊断可用 `LLMUSAGE_LOG=info` 记录定价阶段边界，或用 `debug` 记录页进度；默认 `warn` 级别会在重算超过 30 秒后记录一次存活告警。

## recent-ready 信号

```powershell
llmusage sync --recent-days 1
```

`--recent-days` 为调用方启用 recent-window 信号。当前 parser 表面仍会按需扫描已有 cursor，以保持正确性。

## 安全重建

```powershell
llmusage sync --rebuild
```

`--rebuild` 会按来源重置 parser-backed 用量状态，再重新解析本地真源。parserless Antigravity 的 event、bucket、行为事实、cursor 和 source-file 诊断都会保留。如果 parser 来源的已导入文件型历史依赖现在缺失的源文件，默认拒绝执行。

Token 统计口径按 parser 来源单独记录版本。含旧口径行的数据库仍可读取，但普通
sync 会拒绝混写新旧结果。请逐个显式重建受影响来源：

```powershell
llmusage sync --rebuild --source codex
llmusage sync --rebuild --source claude
llmusage sync --rebuild --source opencode
```

只有重建完整成功后才会推进来源 marker。来源仍需重建时，`source-status` 和
diagnostics 会返回 `legacy_token_accounting`、`token_accounting_version` 和可执行的警告信息。

`llmusage serve` 会在绑定 Dashboard 端口前自动修复可安全迁移的旧版 parser 来源。
存在有损重建风险的来源会告警并跳过：历史报表仍可读取，普通写入继续被 guard 拒绝，
Dashboard 仍会启动。已通过安全预检的来源若发生 parser、SQLite 或提交错误，Dashboard
会停止启动。自动修复永远不会启用 `--allow-lossy-rebuild`。

只有明确接受清掉不可重建历史时才使用：

```powershell
llmusage sync --rebuild --allow-lossy-rebuild
```

建议先诊断：

```powershell
llmusage diagnostics --out .\llmusage-diagnostics.json
```

## sync 写入什么

- `usage_event`：标准化来源事件。
- `usage_bucket_30m`：报表和 Dashboard 使用的 30 分钟 UTC 聚合。
- `usage_turn` 与 `usage_tool_call`：隐私边界内的行为事实。
- `source_file`：用于诊断的 live/missing/deleted 源文件状态。
- `source_cursor`：增量 cursor。
- `run_log` 与 `source_sync_status`：运行状态。

Token 质量标签来自来源 descriptor，不是运行时猜测：`precise` 来源保留 input、output、cache read、cache creation/write、reasoning 和 total 通道；`total_only` 不声称子通道精确；`estimated` 明确表示近似；仅监控或阻塞来源会显示为 unavailable/parserless，而不是被导入。

对 precise 来源，`input_tokens` 表示非缓存输入，缓存通道单独展示，parser 写入的
`total_tokens` 是报表和 Dashboard 的唯一总量依据。Reasoning 是诊断子通道；当上游
output 或 total 已包含它时，不会再次相加。
