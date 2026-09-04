# llmusage

[English](./README.md) · [文档](https://bahayonghang.github.io/llmuasage/zh/)

> **命名说明：** crate 与二进制文件名为 `llmusage`；GitHub 仓库名为 `llmuasage`（多一个 `a`）。托管文档的链接使用仓库拼写。

本地优先的 AI CLI 用量分析工具。`llmusage` 会被动读取本机 Codex、Claude Code、OpenCode、Kimi Code、Pi、Oh My Pi、Grok Build、ZCode、Antigravity CLI 和 DeepSeek Harness 的本地记录，并写入本地 SQLite；随后提供命令行报表、终端 Dashboard、浏览器 Dashboard 和离线 HTML 导出，默认不上传本地用量。`dash` 的 Usage 页会用本机已有 CLI 凭证读取订阅额度。

> 当前 crate 版本：`1.3.0`。

![llmusage 本地 Web Dashboard 概览](./docs/public/screenshots/web-dashboard-overview.png)

<small>截图来自 `llmusage serve` 启动的脱敏本地 fixture，不是真实用户数据。</small>

## 安装

```powershell
cargo install llmusage --git https://github.com/bahayonghang/llmuasage.git
```

开发时可在当前 checkout 中用 `just install` 安装，或用 `cargo run` 直接运行：

```powershell
just install
cargo run -- --help
```

已安装版本可通过 Cargo 从官方仓库更新：

```powershell
llmusage update --check
llmusage update
llmusage update dev
```

`update` 需要本机已安装 Git、Rust 和 Cargo。默认 `main` 渠道会从官方仓库
解析最高稳定 release tag，显示 tag 与 commit，并通过 Cargo 的 `--rev` 安装该
不可变 commit。确认后命令会再次解析目标；目标发生变化时拒绝继续。
`llmusage update dev` 会显示当前 commit，但仍明确跟踪可变的 `dev` 分支，不是
经过稳定发布验证的版本。`--check` / `-c` 会联网解析官方 refs 并打印计划，
但绝不启动 Cargo。

顶层 help 现在使用表格形式，方便快速浏览。中文顶层 help 可用 `llmusage help --zh`；子命令旧版 clap help 仍可用 `llmusage help <COMMAND>` 或 `llmusage <COMMAND> --help`。

默认运行时目录是 `~/.llmusage/`。可用 `--home <PATH>` 或 `LLMUSAGE_HOME` 覆盖。
结构化运行日志只写本地 NDJSON 分片：`~/.llmusage/logs/llmusage.ndjson.*`。文件日志可用 `LLMUSAGE_LOG=off|error|warn|info|debug|trace` 控制（默认 `warn`）；`RUST_LOG` 继续只控制控制台 stderr 日志。分片在进程运行期间达到 10 MiB 即轮转，总量最多保留 30 MiB、7 个文件和 7 天。

## 最短路径

```powershell
llmusage init
llmusage sync
llmusage
llmusage serve
```

含义：

1. `init` 创建 `~/.llmusage/` 并初始化 `llmusage.db`，不会修改第三方工具配置。
2. `sync` 被动、增量解析本地真源，写入 usage 行、30 分钟 bucket、source-file 诊断和行为事实；本地 driver 之后还会拉取已注册的 SSH 远端。无界 sync 还会先告警，并在写入新数据前自动重建可无损修复的旧版 token accounting 来源。
3. `llmusage` 显示默认 daily 报表：所选时区下最近 7 个自然日。
4. `serve` 会按需安全重建旧版 parser token 统计口径，然后默认在 `127.0.0.1` 启动浏览器 Dashboard。只有明确需要远程访问时才使用 `serve --public`：它会暴露不带认证和 TLS 的聚合 Dashboard，但 project label、日志、诊断、job 状态和所有写路由仍只允许本地访问。

内置定价目录升级后的第一次 sync 会在扫描来源前重算历史事件价格。stderr 会显示目录版本、已处理/总事件数、汇总桶对账和完成状态；`sync --json-events` 会在纯 NDJSON stdout 中提供同一套定价生命周期。

## 支持的本地来源

| 来源 | 本地记录 / 状态 |
| --- | --- |
| Codex | OpenAI Codex rollout/session JSONL |
| Claude | Claude Code project JSONL |
| OpenCode | OpenCode 本地 SQLite 用量库 |
| Antigravity | `~/.gemini/antigravity-cli/conversations/*.db`（或 `GEMINI_CLI_HOME`）；hook 时代的历史行继续可查，存在未归属历史时拒绝 rebuild |
| Kimi Code | `~/.kimi-code/sessions/**/wire.jsonl`（或 `KIMI_CODE_HOME`），只读取 turn-scoped `usage.record` |
| Pi | `~/.pi/agent/sessions/**/*.jsonl`（或 `PI_AGENT_DIR`），来源 id 为 `pi` |
| Oh My Pi | `~/.omp/agent/sessions/**/*.jsonl`，来源 id 为 `omp`。路径重叠时归 `pi`。升级后第一次不带 `--source` 的 `sync` 会重建存量 `pi` 行。后续 provider/project/成本/行为回填使用 `sync --rebuild --source omp`。 |
| Grok Build | `~/.grok/sessions/*/*/`（或 `GROK_HOME`），只读取会话根目录的 `updates.jsonl`、`signals.json`、`summary.json` 和可选 `events.jsonl` sidecar |
| ZCode | `~/.zcode/cli/db/db.sqlite`（或 `ZCODE_HOME`）中 `model_usage` 的 completed 行 |
| DeepSeek Harness | `~/.dsh/sessions/**/session.jsonl.zstd` 或 `session.jsonl`（或 `DSH_HOME`）；按帧魔数分派压缩与否 |

Kimi Code、Pi、Oh My Pi、ZCode、Antigravity CLI、DeepSeek Harness 和 Grok Build 都是 passive、`precise` 来源：保留原始模型名，通过来源级 cursor 保证增量与幂等重放，且不持久化 transcript 正文。Pi 与 Oh My Pi 共用一份解析实现，游标分开。Pi 支持由本机 Oh My Pi 样本和脱敏 Pi-compatible fixture 共同验证；Pi-only 的本机证据仍有限。Grok Build 把每条 `turn_completed` 的 `params.update.usage` 映射为一条事件（input、cache read、cache creation、output、诊断 reasoning、权威 total）。没有 usage 的会话仍走旧的 total-only 回退。任一 sidecar 变化时按会话整体重放。定价目录没有 grok 行，且不使用 `costUsdTicks`，成本保持 `unpriced`。`source-status` 和 `dash` 还会显示 Reasonix、Gemini CLI、Cursor、Copilot、Zed、Kiro、Goose、Kimi shell/Qwen、Roo/Kilo/Cline、Codebuff、Crush、Warp/Oz、Amp、Hermes、Trae 等仅监控平台。仅监控表示 llmusage 可以探测候选本地路径并说明为什么阻塞解析；不会写入 0 用量行，也不会写入未验证 token 行。

从曾安装 hook/plugin 的旧版本升级后，应执行一次 `llmusage uninstall`。该命令只清理 llmusage 自有的遗留条目和 wrapper，保留历史备份与用量数据；`--purge` 才会额外删除整个运行时根目录。

## 常用命令

```powershell
llmusage daily --source codex --since 20260501 --until 20260518
llmusage weekly --sections daily,monthly --no-cost
llmusage codex daily --since 2026-05-01 --until 2026-05-18
llmusage monthly --breakdown
llmusage session --project my-repo
llmusage blocks --active
llmusage source-status
llmusage remote add devbox user@devbox
llmusage help --zh
llmusage dash
llmusage codex-tracer
llmusage logs --limit 50 --level warn
llmusage catalog status
llmusage update --check
llmusage export html --out .\llmusage-report
```

报表命令只是只读 SQLite 查询；如果数据库过旧，先运行 `llmusage sync`。

## 报表

`daily`、`weekly`、`monthly` 和 `session` 共用 coding-agent 报表形状。人读表格展示聚合 `All` 行和按来源拆分的 `Agent` 行；CLI JSON 使用 camelCase 字段，`--by-agent` 会把嵌套来源行加入 JSON。`weekly` 按每周周一的起始日期分组。

报表日期筛选同时接受 `YYYYMMDD` 和 `YYYY-MM-DD`。`--sections daily,weekly,monthly,session` 可以在一次输出中组合多个周期段（当前命令周期始终排在最前）；`--no-cost` 会隐藏成本列与 JSON 成本字段，但不会改变 token 总量。

单来源视图使用 `llmusage <source> <period>`，例如 `llmusage claude daily` 或 `llmusage codex monthly`。支持的 source host 是 `claude`、`codex`、`opencode` 和 `antigravity`，每个都支持 `daily`、`weekly`、`monthly`、`session`。它与 `<period> --source <source>` 的数据相同，但会从文本和 JSON 移除 Agent 对比层。`blocks` 有意继续作为顶层命令。这个均匀来源 surface 是 llmusage 的扩展，不表示每个来源都复刻 ccusage 的逐来源能力矩阵。

`llmusage dash` 使用 tokscale 风格的终端 Dashboard。快捷键：`tab`/`shift-tab` 或 `1`-`9` 切换视图；`j`/`k`、方向键、Page Up/Page Down、Home/End 或鼠标滚轮选择行；`o` 循环可排序列，`O` 反转排序方向；`s` 打开来源选择器；`r` 刷新 Dashboard 数据；`R` 切换自动刷新；`x` 按当前来源筛选运行 sync；`?` 打开帮助/设置；`q` 退出。

浏览器看板包含行为分析面板和本地用量分析工作台，可按时间、指标和分组维度分析用量，并支持工具/非工具成本归因与离线快照导出。

## 桌面应用

Windows 桌面应用读取与 CLI 相同的本地用量数据库。

```powershell
just desktop-dev
just desktop-build
```

`just desktop-dev` 从 `desktop/` 启动 Tauri 开发壳。
`just desktop-build` 在 `desktop/src-tauri/target/release/bundle/nsis/` 写出未签名 NSIS 安装包。
安装包未做代码签名，Windows SmartScreen 可能告警。

## 模型价格目录

模型价格和上下文窗口来自内置 `static-v2` 目录。该目录已为 Codex 和 OpenCode 加入 `gpt-5.6-luna`、`gpt-5.6-terra`、`gpt-5.6-sol`，其中 `gpt-5.6` 是 Sol 的精确别名；单请求提示 token 超过 272,000 时使用长上下文费率。

可以只写增量覆盖，不需要复制整份内置目录：

```powershell
llmusage catalog apply .\pricing-overlay.json
llmusage catalog status --json
llmusage catalog reset
```

覆盖层按稳定模型 id 新增、完整替换或删除模型定义。apply/reset 会重算已落库 event 成本和 30 分钟 bucket 定价。`doctor --refresh-pricing <PATH>` 继续作为完整 base snapshot 的兼容入口，不是增量覆盖。所有目录输入都必须是本地文件，llmusage 不会联网拉取价格。

将 `LLMUSAGE_LOG` 设为 `info` 可在本地文件日志中记录定价重算的开始、对账和完成；页级记录需要 `debug`。终端人读进度不依赖文件日志级别；重算超过 30 秒后会按默认 `warn` 级别记录一次仍在推进的告警。

## Codex Tracer

```powershell
llmusage codex-tracer
llmusage codex-tracer --port 9876
llmusage codex-tracer --no-open
llmusage codex-tracer --rebuild
```

`codex-tracer` 是一个只面向 Codex 的本地 Dashboard。它会从 `$CODEX_HOME/rollout/` 或 `~/.codex/rollout/` 读取 rollout JSONL，构建独立的 `~/.llmusage/codex-tracer.db`，然后启动带细粒度 token 会计和线程追踪的专用浏览器界面。

## 安全默认值

- 不需要账号登录、device token、上传队列或远端用量 API。SSH 远端导入是你触发的、从已注册主机拉取规范化字段，不会上传用量。
- 普通无界 `llmusage sync` 只会在全部目标都通过无损预检后，自动重建所选的旧版 token accounting 来源；任一目标不安全时，不会 reset 任何自动修复目标。
- 普通 `llmusage sync` 遇到原始源文件缺失时会保留已导入 usage。
- `llmusage sync --recent-days N` 只导入最近的 UTC 事件窗口（`1..=3650`），且不推进全历史 cursor；`--parallelism` 合法范围为 `1..=32`。
- bounded sync 不会自动重建旧版 accounting，因为清空全历史后只导入时间窗口会造成丢失；请先运行无界 `llmusage sync`。
- `llmusage sync --rebuild` 默认拒绝有损重建，除非同时传入 `--allow-lossy-rebuild`。
- 无 source 的 `llmusage sync --rebuild` 会重置 parser-backed 来源。若重建会删除未归属的 hook 时代 Antigravity 行，即使带 `--allow-lossy-rebuild` 也会拒绝。
- `llmusage serve` 也会在绑定端口前自动重建可安全迁移的旧版 parser 来源。与普通 sync 的全量预检不同，serve 只跳过有风险的来源，让只读 Dashboard 仍可启动。
- 自动修复永远不会启用 `--allow-lossy-rebuild`；请先恢复缺失源文件，再显式执行 `llmusage sync --rebuild --source <source>`。
- `llmusage diagnostics --forget-file <PATH> --source <SOURCE>` 是显式忽略源文件的写入入口。
- `llmusage logs` 查询本地运行日志和最近命令审计记录，不改变报表 stdout 或 `sync --json-events` stdout 合同。
- `llmusage serve --public` 只暴露聚合看板的总量、趋势、模型、来源、成本和最小健康状态响应。项目、日志、诊断、任务状态、行为明细、用量分析和写操作必须使用默认回环地址监听，远程场景通常通过 SSH 隧道访问。
- `llmusage catalog apply <file>` 与 `doctor --refresh-pricing <file>` 只读取本地目录文件；URL 会被拒绝。

## 文档

- [指南](./docs/zh/guide/getting-started.md)
- [Codex Tracer 指南](./docs/zh/guide/codex-tracer.md)
- [Dashboard](./docs/zh/dashboard/index.md)
- [CLI 参考](./docs/zh/reference/cli.md)
- [安全说明](./docs/zh/safety/index.md)
- [架构说明](./docs/zh/architecture/index.md)

开发门禁：

```powershell
just ci
```
