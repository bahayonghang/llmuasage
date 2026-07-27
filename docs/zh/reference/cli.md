# CLI 参考

本页按版本 `1.1.0` 的 `cargo run -- --help`、`cargo run -- serve --help`、`cargo run -- export html --help` 对齐。顶层 help 使用紧凑表格；子命令 help 继续使用 clap 输出。

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
| `--since <YYYY-MM-DD\|YYYYMMDD>` | 报表命令的包含式开始日期 |
| `--until <YYYY-MM-DD\|YYYYMMDD>` | 报表命令的包含式结束日期 |
| `--json` | 支持的报表命令输出稳定 JSON |
| `--breakdown` | 在支持处包含按模型拆分的行或 payload |
| `--order asc\|desc` | 按周期/活动排序报表行 |
| `--timezone UTC\|local\|+08:00` | 报表时区。`local` 使用本机当前固定本地偏移，不是 IANA/DST 感知时区。 |
| `--locale <LOCALE>` | 标题和数字格式的轻量 locale 选择 |
| `--compact` | 使用更窄的表格布局 |
| `--no-cost` | 从报表输出隐藏成本列与成本字段 |
| `--source codex\|claude\|opencode\|antigravity\|kimi_code\|pi\|grok` | 将顶层报表或同步命令限制到一个来源 |
| `-A, --by-agent` | 在统一报表 JSON 中加入嵌套来源行 |
| `--sections daily\|weekly\|monthly\|session` | 在一次组合输出中加入报表周期 |
| `--all` | daily 显示完整历史，而不是默认最近 7 天 |
| `--instances` | daily 按项目/实例分组 |
| `--project <PROJECT>` | 按项目 label、hash 或 reference 过滤 |

## 运行时日志

`llmusage` 默认把结构化运行诊断写到本地 NDJSON 分片 `~/.llmusage/logs/llmusage.ndjson.*`，每行一个 JSON 对象。

| 环境变量 | 含义 |
| --- | --- |
| `LLMUSAGE_LOG=off\|error\|warn\|info\|debug\|trace` | 控制本地 NDJSON 日志文件；默认 `warn` |
| `RUST_LOG=...` | 继续控制控制台 stderr 日志 |

文件日志不会写入报表 stdout，也不会改变 `sync --json-events` stdout。分片在进程运行期间达到 10 MiB 即轮转，总量最多保留 30 MiB、7 个文件和 7 天；`logs` 与 `diagnostics` 状态会包含保留文件/字节数、队列丢弃事件数和维护失败数。

## 报表命令

报表命令只读取本地数据库。

### `llmusage` / `llmusage daily`

```powershell
llmusage
llmusage daily --all
llmusage daily --source codex --since 20260501 --until 20260518
llmusage daily --json --breakdown
```

默认命令。展示 daily token 与估算成本。daily、weekly、monthly 的文本输出采用统一的 `All` 加 `Agent` 行；CLI JSON 使用 camelCase，传入 `--by-agent` 时会加入嵌套来源行。

### `llmusage weekly`

```powershell
llmusage weekly
llmusage weekly --since 2026-05-04 --until 2026-05-10
llmusage weekly --by-agent --json
```

按每周周一的起始日期分组。

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

### `llmusage <source> <period>`

```powershell
llmusage claude daily
llmusage codex monthly --json
llmusage opencode weekly --no-cost
llmusage antigravity session
```

`claude`、`codex`、`opencode`、`antigravity` 都挂载 `daily`、`weekly`、`monthly`、`session`。聚焦命令会注入对应来源筛选，数据与 `<period> --source <source>` 相同，并移除 `Agent`/`Detected` 对比层。其 JSON 不含 `agent` 或 `agents` 字段。重复传入同值 `--source` 可以接受；冲突值会被拒绝。`blocks` 有意不在这个命令树中。

这是 llmusage 的均匀扩展，不是逐来源复刻 ccusage 的能力矩阵。

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

创建本地运行时并初始化用量数据库，不修改第三方工具配置。

### `llmusage sync`

```powershell
llmusage sync
llmusage sync --source codex
llmusage sync --source kimi_code
llmusage sync --source pi
llmusage sync --source grok
llmusage sync --recent-days 1
llmusage sync --recent-days 30 --parallelism 4
llmusage sync --json-events
llmusage sync --rebuild
llmusage sync --rebuild --allow-lossy-rebuild
```

`--source`、`--recent-days` 与 `--parallelism` 和 `POST /api/jobs`、公开 `JobRegistry` API 共用同一校验契约。非法值分别返回稳定错误码 `unknown_source`、`invalid_recent_days` 或 `invalid_parallelism`。

导入本地来源。扫描来源前，bootstrap 可能升级未固定的内置定价目录并重算历史事件价格。普通无界 sync 还会检测所选的旧版 token-accounting 来源，先告警，并且只在全部目标都通过无损预检后自动重建；一个风险目标会阻止全部自动 reset。存在旧版 accounting 时，bounded `--recent-days` 请求必须先运行一次无界 sync。

人读 stderr 会显示目录版本、已处理/总事件数、汇总桶对账、token-accounting 自动修复边界和完成耗时。`--json-events` 在纯 NDJSON stdout 写同一生命周期，包括新增的 `token_accounting_repair_started` / `token_accounting_repair_finished` 和既有 pricing 事件。目录已是最新或固定了 snapshot/overlay 时不会输出 pricing 事件；accounting 已是当前版本时不会输出 repair 事件。`--allow-lossy-rebuild` 必须显式配合 `--rebuild`，普通 sync 永远不会推断该授权。

设置 `LLMUSAGE_LOG=info` 可记录结构化的定价开始/对账/完成文件日志，`debug` 还会记录节流后的页进度。默认 `warn` 级别会在重算持续超过 30 秒时记录一次存活告警；终端进度不受文件日志级别影响。

人读 stdout 摘要只输出一张对齐表格：每个来源一行，并以 `TOTAL` 收尾；已完成进度留在 stderr，不再成为重复的永久成功行。表格按来源显示 `files`、`changed`、`skipped`、`seen`、`committed`、`stored_events`、bytes 和 parse/write 耗时。`skipped` 对文件型来源来自现有 cursor/fingerprint 证据，对 OpenCode 这种 DB 来源来自 SQLite 高水位 cursor；`committed` 是 SQLite 去重后本次新增写入数。重定向输出不含 ANSI，窄终端使用紧凑表头且不截断数值。

## 状态与诊断

### `llmusage status`

```powershell
llmusage status
```

输出人读的数据库、来源和最近运行摘要。

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

快捷键：`tab`/`shift-tab` 或 `1`-`9` 切换视图；`j`/`k`、方向键、Page Up/Page Down、Home/End 或鼠标滚轮选择行；`o` 循环可排序列，`O` 反转排序方向；`h`/`l` 在适用视图切换时间窗口；`s` 打开来源选择器；`r` 刷新 Dashboard 数据；`R` 切换自动刷新；`x` 通过现有 sync worker lock 按当前来源筛选运行 sync；`?` 打开帮助/设置；`q` 退出。

### `llmusage serve`

```powershell
llmusage serve
llmusage serve --port 37421
llmusage serve --public --no-open --port 37421
```

默认在 `127.0.0.1` 启动完整 Web Dashboard 和本地 JSON API。`--public` 会绑定 `0.0.0.0`，但只暴露只读聚合 Dashboard allowlist（`/`、静态资源、`/api/dashboard` 和 `/api/health`）；projects、日志、diagnostics、jobs、行为/Explorer 明细和写操作仍只限 loopback。public 聚合视图不提供认证或 TLS。`--no-open` 会关闭浏览器启动；SSH 会话也会自动跳过浏览器启动。远程需要完整本地 API 时，应通过 SSH 隧道访问 loopback 监听。

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

清理由旧版 llmusage 留下的 hook、plugin、wrapper 和原子写入残留。只移除 llmusage 自有条目，保留同级用户配置与历史备份；无清理对象时不会创建备份或 integration 审计行。从会安装 hook 的旧版本升级后，应执行一次本命令。`--purge` 还会删除运行时根目录及其中的用量数据库。
