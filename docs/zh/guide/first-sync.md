# 第一次同步

`llmusage sync` 会把本地用量导入 SQLite。报表命令不会自动 sync；数据库过旧时需要先同步。

## 导入所有来源

```powershell
llmusage sync
```

人读进度写入 stderr，最终摘要保留在 stdout。

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

该模式会在 stdout 输出 NDJSON 生命周期/进度事件，适合 wrapper 或 UI adapter 使用。

## recent-ready 信号

```powershell
llmusage sync --recent-days 1
```

`--recent-days` 为调用方启用 recent-window 信号。当前 parser 表面仍会按需扫描已有 cursor，以保持正确性。

## 安全重建

```powershell
llmusage sync --rebuild
```

`--rebuild` 会先清空可重建 usage 行、bucket、project 和 cursor，再重新解析本地真源。如果已导入的文件型历史依赖现在缺失的源文件，默认拒绝执行。

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
