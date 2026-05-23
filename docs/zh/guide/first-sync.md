# 第一次同步

`llmusage sync` 会把本地用量导入 SQLite。报表命令不会自动 sync；数据库过旧时需要先同步。

## 导入所有来源

```powershell
llmusage sync
```

人读进度写入 stderr，最终摘要保留在 stdout。

## 只导入一个来源

```powershell
llmusage sync --source codex
llmusage sync --source claude
llmusage sync --source opencode
llmusage sync --source gemini
# antigravity 可作为输入别名；输出/source id 仍为 gemini
```

合法来源与 `cargo run -- --help` 一致：`codex`、`claude`、`opencode`、`gemini`（`antigravity` 是 Google 来源别名）。

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
