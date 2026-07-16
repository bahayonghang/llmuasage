# CLI 参考

本页按版本 `0.9.2` 的 `cargo run -- --help`、`cargo run -- serve --help`、`cargo run -- export html --help` 对齐。顶层 help 使用紧凑表格；子命令 help 继续使用 clap 输出。

## 顶层 help

```powershell
llmusage help
llmusage --help
llmusage -h
llmusage help --zh
```

`llmusage help`、`llmusage --help`、`llmusage -h` 输出英文表格 help；`llmusage help --zh` 输出中文表格 help。子命令旧版 clap help 仍使用 `llmusage help <COMMAND>` 或 `llmusage <COMMAND> --help`。

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
| `--timezone UTC\|local\|+08:00` | 报表时区。`local` 使用本机当前固定本地偏移，不是 IANA/DST 感知时区。 |
| `--locale <LOCALE>` | 标题和数字格式的轻量 locale 选择 |
| `--compact` | 使用更窄的表格布局 |
| `--source codex\|claude\|opencode\|antigravity` | 报表或同步限制到一个来源 |
| `--all` | daily 显示完整历史，而不是默认最近 7 天 |
| `--instances` | daily 按项目/实例分组 |
| `--project <PROJECT>` | 按项目 label、hash 或 reference 过滤 |

## 运行时日志

`llmusage` 默认把结构化运行诊断写到 `~/.llmusage/logs/llmusage.ndjson`。该文件只保存在本地，每行一个 JSON 对象。

| 环境变量 | 含义 |
| --- | --- |
| `LLMUSAGE_LOG=off\|error\|warn\|info\|debug\|trace` | 控制本地 NDJSON 日志文件；默认 `warn` |
| `RUST_LOG=...` | 继续控制控制台 stderr 日志 |

文件日志不会写入报表 stdout，也不会改变 `sync --json-events` stdout。初版保留一个活动日志文件；启动时如果超过 10 MiB，会轮转为 `llmusage.ndjson.old`。

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
llmusage sync --source antigravity
llmusage sync --recent-days 1
llmusage sync --json-events
llmusage sync --rebuild
llmusage sync --rebuild --allow-lossy-rebuild
```

导入本地来源。`--json-events` 在 stdout 写 NDJSON 生命周期事件。`--allow-lossy-rebuild` 必须配合 `--rebuild`。

人读摘要会按来源显示 `files`、`changed`、`skipped`、`seen`、`committed` 和 `stored_events`。`skipped` 对文件型来源来自现有 cursor/fingerprint 证据，对 OpenCode 这种 DB 来源来自 SQLite 高水位 cursor。`committed` 是 SQLite 去重后本次新增写入数。

## 状态与诊断

### `llmusage status`

```powershell
llmusage status
```

输出人读的数据库、来源、集成和最近运行摘要。

### `llmusage source-status`

```powershell
llmusage source-status
```

输出解析器支持的来源与仅监控平台状态。

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

执行健康检查。`--refresh-pricing <PATH>` 会校验完整的内部 v1、catalog v2 或原生 LiteLLM base snapshot，在 `~/.llmusage/pricing/` 下保存内容寻址副本，清除当前 overlay，并重算 event 成本；URL 会被拒绝。该参数替换完整 base，不是增量覆盖。

### `llmusage catalog`

```powershell
llmusage catalog apply .\pricing-overlay.json
llmusage catalog status
llmusage catalog status --json
llmusage catalog reset
```

`catalog apply` 校验并激活本地 v2 overlay。overlay 始终与记录的 base 合并，因此第二次 apply 不会叠加在上一个 effective catalog 上。已有 `id` 的模型会被完整替换；`remove_models` 引用未知 id 时会失败。激活会保存内容寻址的 base/overlay/effective 文件，先重算已落库 event 和 bucket 成本，再切换 catalog metadata。

`catalog status` 区分 base、可选 overlay 和 effective catalog。JSON 输出包含每层声明版本、运行时身份、schema 版本、文件、模型数、展开后的来源规则数和 `rebase_available`。

`catalog reset` 移除 overlay 并恢复它记录的 base。snapshot base 会继续固定；embedded base 会回到当前二进制内置目录。没有 overlay 时 reset 幂等成功。

最小 overlay：

```json
{
  "schema_version": 2,
  "kind": "overlay",
  "version": "team-pricing-2026-07",
  "models": [
    {
      "id": "team-model",
      "sources": ["codex", "opencode"],
      "matches": [
        { "value": "team-model", "mode": "exact" }
      ],
      "rates": {
        "default": {
          "input_per_mtok": 1.0,
          "cached_per_mtok": 0.1,
          "cache_creation_per_mtok": 1.25,
          "output_per_mtok": 6.0
        },
        "tiers": [
          {
            "name": "long_context",
            "prompt_tokens_above": 272000,
            "input_per_mtok": 2.0,
            "cached_per_mtok": 0.2,
            "cache_creation_per_mtok": 2.5,
            "output_per_mtok": 9.0
          }
        ]
      },
      "context_window": 1050000
    }
  ],
  "remove_models": []
}
```

`exact` 只匹配规范化后的完整模型 id；`family` 还接受 dash/dot 规范化后的家族后缀。exact 优先于 family，同模式下最长 matcher 优先。`version` 只用于审计，不控制文件路径。tier 阈值按单条 `usage_event` 的 input + cache-read + cache-creation token 选择；bucket 总量不会再次触发 tier。

### `llmusage logs`

```powershell
llmusage logs
llmusage logs --limit 50 --level warn
llmusage logs --command sync --json
```

查询本地结构化运行日志和 SQLite `run_log` 最近命令记录。过滤条件会应用到本地运行日志文件和 `run_log` 命令标签；不会倾倒 usage raw JSON、prompt 或 response。

## 本地界面命令

### `llmusage dash`

```powershell
llmusage dash
```

交互式终端 Dashboard。旧的隐藏 `tui` 命令是已废弃别名。

快捷键：`tab`/`shift-tab` 或 `1`-`8` 切换视图，`j`/`k` 或方向键滚动，`h`/`l` 在适用视图切换时间窗口，`s` 打开来源选择器，`r` 刷新 Dashboard 数据，`R` 切换自动刷新，`x` 通过现有 sync worker lock 按当前来源筛选运行 sync，`?` 打开帮助/设置，`q` 退出。

### `llmusage serve`

```powershell
llmusage serve
llmusage serve --port 37421
```

在 `127.0.0.1` 启动本地 Web Dashboard 和 JSON API。

### `llmusage codex-tracer`

```powershell
llmusage codex-tracer
llmusage codex-tracer --port 9876
llmusage codex-tracer --no-open
llmusage codex-tracer --rebuild
```

启动只面向 Codex 的本地 Dashboard。它会从 `$CODEX_HOME/rollout/` 或 `~/.codex/rollout/` 读取 rollout JSONL，写入独立的 `codex-tracer.db`，并提供带细粒度 token 会计和线程追踪的专用浏览器界面。

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
