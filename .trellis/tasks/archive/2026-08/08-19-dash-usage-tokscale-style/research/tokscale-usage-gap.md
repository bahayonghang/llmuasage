# tokscale Usage vs llmusage dash Usage

对照日期：2026-08-19

对照来源：用户两张截图、`ref/repo/tokscale/crates/tokscale-cli/src/commands/usage/`、`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/usage.rs`、`src/tui/panels/usage.rs`、`Dashboard::sync_command_center`。

## 结论

两页共用 tab 名 `Usage`，数据域不同。

- tokscale Usage：本机凭证 + 云端订阅额度。
- llmusage Usage：本地 SQLite 同步指挥中心。

现有 `SyncCommandCenterPayload` 不能填 tokscale 的 Account / Plan / Auth / Health / Limit / Reset。

## tokscale Usage

源码：`crates/tokscale-cli/src/tui/ui/usage.rs`（约 2200 行）、`crates/tokscale-cli/src/commands/usage/mod.rs`。

### 布局

| 宽度/高度 | 构图 |
| --- | --- |
| `>=132` 且 `>=20` | 上半：Usage Summary \| Selected Account；下半：Accounts 表 |
| `>=104` 且 `<132` | 纵向：Summary → Selected Account → Accounts |
| `<104` 或 `<20` | Accounts 表 + 底部 7 行 Selected Account |

外框标题 `Usage`。右上角状态如 `2 providers · 2 managed · 2 issues`。

### 操作条

`r Refresh`、`a Add Codex`、`m Show/Hide Emails`。选中账号有 reset credit 时再出 `x Reset`。

进入 Usage tab 且尚未拉过时自动 `fetch_subscription_usage`。`R` 自动刷新只轮询额度，不重载本地用量。

### Usage Summary

键值行：State、Active、Capacity、Fallback、Next Reset、Action。

后面按高度追加：Diagnostics、Credit Bank（Codex reset）、Attention、Providers。

截图中的诊断示例：

- `Claude: Claude usage request failed (HTTP 429 Too Many Requests)`
- `Amp: Amp returned no parseable usage (display_text format may have changed)`

### Selected Account

Status、Email、Credential、Limits（进度条 + remaining + reset）、Snapshot、Actions。

外部 CLI 凭证显示 `managed externally`。邮箱可用 `m` 打码。

### Accounts 表

宽表列：`#`、Provider、Account、Plan、Auth、Health、Limit、Reset。

窄表两行：`#  Account / Status`。

Health 由剩余额度派生：`>=25%` Ready，`<25%` Watch，`<10%` Critical，无 metric 则 Unknown。

### 数据面

统一 DTO：`UsageOutput { provider, account, credential_source, plan, email, metrics[], reset_credits, credit_status, spend_control }`。

`UsageMetric { label, used_percent, remaining_percent, remaining_label, resets_at }`。

失败进入 `UsageFetchDiagnostic`，账号行仍保留。

提供者（有本机凭证才拉）：

| Provider | 凭证来源 |
| --- | --- |
| Claude | `~/.claude/.credentials.json` 或 macOS Keychain |
| Codex | Codex credential store / 当前 login / OpenCode auth |
| Z.ai | 本地 Z.ai 凭证 |
| Amp | 本地 Amp 凭证 |
| Copilot | `gh` 登录态 |
| Grok Build | `~/.grok/auth.json` 或 `GROK_HOME/auth.json` |
| Kimi | `~/.kimi-code/credentials/kimi-code.json` 或 `~/.kimi/credentials/` |
| MiniMax / MiniMax Token Plan | 本地 MiniMax 凭证 |
| Warp/Oz | 本地 Warp 凭证 |
| Sakana | 本地 Sakana 凭证 |

`fetch_all` 在 `thread::scope` 里并行请求。5 分钟 JSON 缓存：`subscription-usage-cache.json`。

Claude 路径只读凭证，不写回。Kimi 过期会 refresh 并写回 token。Codex TUI 路径会 import 当前 login，并支持 Add / Use / Remove / Reset。

## 当前 llmusage Usage

源码：`src/tui/panels/usage.rs`、`src/tui/data_loader.rs:121`、`src/query/mod.rs` 的 `SyncCommandCenterPayload`。

`Panel::Trends` 的显示名是 `Usage`。加载的是 `Dashboard::sync_command_center`，不是订阅额度。

### 布局

标题 `Usage / Sync`。

1. 4 行摘要：headline + reason、events/inserted/stored/sources、lock/rebuild-risk/monitored/parserless、last run。
2. `Source Sync` 表：Source、Status、Seen、Inserted、Skipped、Stored、Issues、Share、Updated。
3. 高度 `>=16` 时底部 `Platform Monitor`。

`x` 启动本地 sync。`r` 刷新 SQLite 查询。页脚仍是 `source · window · tokens · $cost`。

测试锁文案：`tests/tui_panels_prop.rs` 的 `usage_panel_renders_sync_status_and_platform_monitor_summary`。

web dashboard 也消费 `sync_command_center`。本任务若只改 TUI，不能改该 JSON。

## 产品边界

`README.md` / `README.zh-CN.md`：本地优先；不上传、不登录、不调用云端用量 API。

`Cargo.toml` 描述：`zero upload`。

`CHANGELOG.md`：未加入 upload queue、login、device token、remote pricing fetch。

`llmusage` 根 `Cargo.toml` 没有 `reqwest` / `ureq`。现有网络面是 `serve` 入站和 `update` 的 `git ls-remote`。dash 面板查询只读本地 SQLite。

已归档 `06-12-tokscale-collection-tui-migration` 把 remote usage、billing login、tokscale 远程 API 列为 Out of Scope。当时把 Usage 做成 Usage/Sync，是刻意映射。

已归档 `08-18-dash-models-tokscale-style` 与 `08-19-dash-overview-tokscale-style` 只改本地查询的构图与着色，不引入云端 API。

## 可复用

- TUI 主题槽、`ScrollState`、`selection_style`、英文文案、`NO_COLOR` 合同。
- 本机已有 Grok / Kimi / Claude / Codex 工件路径，与 tokscale 凭证路径重叠。
- tokscale 的 `UsageOutput` / diagnostic / readiness 阈值可作为对照合同。
- Overview / Models 任务的 TestBackend 验收方式。

## 缺口

1. 没有订阅额度 DTO、缓存、fetcher、诊断类型。
2. 没有出站 HTTP 客户端给用量 API。
3. 没有读取 / 打码邮箱、凭证来源标签、进度条 remaining。
4. 没有 Add Codex / reset credit / 账号 store 写路径。
5. Usage 测试锁死 Source Sync / Platform Monitor。
6. `x` 在 llmusage 是 sync；在 tokscale Usage 上是 Reset。
7. 产品文案禁止云端用量 API。

## 不能用本地 sync 伪造的字段

| tokscale 列 | 本地 sync 里没有的事实 |
| --- | --- |
| Plan | 订阅档位 |
| Auth | 凭证托管方式 |
| Health / remaining % | 云端窗口剩余 |
| Limit 进度条 | 云端限额 |
| Reset 时刻 | 云端重置时间 |
| Email | 账号身份 |
| Diagnostics HTTP 429 | 远端请求失败 |
