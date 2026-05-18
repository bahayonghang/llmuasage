# CLI 参考

本页按版本 `0.6.3` 的 `cargo run -- --help`、`cargo run -- serve --help`、`cargo run -- export html --help` 对齐。

## 全局参数

```text
Usage: llmusage [OPTIONS] [COMMAND]
```

| 参数 | 含义 |
| --- | --- |
| `--home <PATH>` | 覆盖 `LLMUSAGE_HOME` 和默认 `~/.llmusage` 运行时根目录 |
| `--since <YYYYMMDD>` | 报表命令的包含式开始日期 |
| `--until <YYYYMMDD>` | 报表命令的包含式结束日期 |
| `--json` | 支持的报表命令输出稳定 JSON |
| `--breakdown` | 在支持处包含按模型拆分的行或 payload |
| `--order asc\|desc` | 按周期/活动排序报表行 |
| `--timezone UTC\|local\|+08:00` | 报表时区 |
| `--locale <LOCALE>` | 标题和数字格式的轻量 locale 选择 |
| `--compact` | 使用更窄的表格布局 |
| `--source codex\|claude\|opencode\|gemini` | 报表或同步限制到一个来源 |
| `--all` | daily 显示完整历史，而不是默认最近 7 天 |
| `--instances` | daily 按项目/实例分组 |
| `--project <PROJECT>` | 按项目 label、hash 或 reference 过滤 |

## 报表命令

报表命令只读取本地数据库。

### `llmusage` / `llmusage daily`

```powershell
llmusage
llmusage daily --all
llmusage daily --source codex --since 20260501 --until 20260518
llmusage daily --json --breakdown
```

默认命令。展示 daily token 与估算成本。

### `llmusage monthly`

```powershell
llmusage monthly --breakdown
```

按月聚合用量。

### `llmusage session`

```powershell
llmusage session
llmusage session --id <ID>
llmusage session --project my-repo
```

按来源 session 聚合。`--id <ID>` 支持精确或部分 session id。

### `llmusage blocks`

```powershell
llmusage blocks --active
llmusage blocks --recent
llmusage blocks --token-limit max
llmusage blocks --session-length 5
```

展示 5 小时用量窗口和 burn-rate 预测。

### `llmusage statusline`

```powershell
llmusage statusline
llmusage statusline --no-cache
llmusage statusline --refresh-interval 10 --cost-source llmusage
```

输出一行适合 statusline 的摘要。

## 设置与同步命令

### `llmusage init`

```powershell
llmusage init
```

创建本地运行时并安装/探测集成。

### `llmusage sync`

```powershell
llmusage sync
llmusage sync --source gemini
llmusage sync --recent-days 1
llmusage sync --json-events
llmusage sync --rebuild
llmusage sync --rebuild --allow-lossy-rebuild
```

导入本地来源。`--json-events` 在 stdout 写 NDJSON 生命周期事件。`--allow-lossy-rebuild` 必须配合 `--rebuild`。

## 状态与诊断

### `llmusage status`

```powershell
llmusage status
```

输出人读的数据库、来源、集成和最近运行摘要。

### `llmusage diagnostics`

```powershell
llmusage diagnostics
llmusage diagnostics --out .\llmusage-diagnostics.json
llmusage diagnostics --forget-file <PATH> --source codex
```

输出机器可读诊断。`--forget-file` 会把源文件标记为 `deleted_by_user` 并移除 cursor 行。

### `llmusage doctor`

```powershell
llmusage doctor
llmusage doctor --json
llmusage doctor --refresh-pricing .\litellm-prices.json
```

执行健康检查。`--refresh-pricing <PATH>` 会把本地 LiteLLM 价格快照复制到 `~/.llmusage/pricing/` 并重算 event 成本；URL 会被拒绝。

## 本地界面命令

### `llmusage dash`

```powershell
llmusage dash
```

交互式终端 Dashboard。旧的隐藏 `tui` 命令是已废弃别名。

### `llmusage serve`

```powershell
llmusage serve
llmusage serve --port 37421
```

在 `127.0.0.1` 启动本地 Web Dashboard 和 JSON API。

### `llmusage export html`

```powershell
llmusage export html
llmusage export html --out .\llmusage-report
```

写入静态 Dashboard bundle。

## 卸载

### `llmusage uninstall`

```powershell
llmusage uninstall
llmusage uninstall --purge
```

恢复被修改的集成文件。`--purge` 还会删除运行时根目录。

## 隐藏命令

`hook-run` 对普通 help 隐藏，由生成的 hook wrapper 调用。
