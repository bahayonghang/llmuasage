# Design: Dash Usage 对齐 tokscale 订阅额度页

## Architecture

本任务新增一条与 SQLite 平行的只读额度管道，并改 TUI Usage 展示层。解析器、store 主键、web `sync_command_center` 不改字段。

```
本机凭证文件（只读）
    → src/subscription/{claude,codex,grok,kimi}.rs
    → 云端用量 API（reqwest / rustls）
    → UsageFetchReport { outputs, diagnostics }
    → ~/.llmusage/cache/subscription-usage.json   # 5 分钟，不含 token
    → TUI Usage 主区域

Dashboard::sync_command_center
    → overlay（原 Usage / Sync 渲染）
```

额度拉取不占用 `TUI_DASHBOARD_QUERY_PERMITS`。

## Boundaries

| 层 | 做 | 不做 |
| --- | --- | --- |
| `src/subscription` | DTO、四家 fetcher、缓存、假服务器测试 | 写凭证、Add Codex、CLI 子命令 |
| Cargo | 增加 `reqwest`（`rustls-tls` + `json`，关掉默认 native-tls） | 引入 tokscale 整棵 usage 树 |
| TUI Usage | 重画成额度页；独立 `QuotaController` | 用 sync 计数填 Limit |
| overlay | `ActiveDialog::SyncStatus` 复用现有 sync 渲染 | 新 tab |
| data_loader | `Panel::Trends` 仍加载 `sync_command_center` 给 overlay | 把 HTTP 放进 query permit |
| web / query | 无字段变化 | 改 `/api/dashboard` |
| theme | 额度条与 Health 走现有 semantic 槽 | 面板写 `Color::*` |

## Data flow

1. 切到 Usage：`QuotaController::fetch_if_needed`。缓存未过期则同步灌入 state；否则后台拉取。同时 `request_panel_data(Trends)` 继续查 `sync_command_center`，供 overlay 使用。
2. `r`：`refresh_panel_data`（SQLite）+ `QuotaController::force_fetch`（绕过缓存）。
3. `R` 的 `needs_refresh` 只走 `refresh_panel_data`，不调用 `QuotaController`。
4. 四家 `has_credentials()` 为真才请求。并行 `tokio::join!` 或 `JoinSet`，每路超时 8s。一路 panic/失败变成 diagnostic，不影响其他路。
5. 成功输出写入 `{LLMUSAGE_HOME}/cache/subscription-usage.json`：`{ "fetched_at": unix_secs, "outputs": [...] }`。文件不含 access / refresh token。
6. `j/k/Pg` 在主区域移动 `ScrollState`（账号行）。打开 overlay 后按键由 dialog 处理，滚动 overlay 内的 Source Sync 表。
7. `m` 只翻转 `hide_usage_emails`，不重拉。
8. 切走 Usage 时保留内存中的额度结果；再次进入若未过期不重拉。`invalidate_inactive_panel_data` 清 `sync_center`，不清额度缓存。

## Contracts

### DTO

```text
UsageMetric { label, used_percent, remaining_percent, remaining_label, resets_at }
UsageAccount { id, label, is_active }
UsageOutput { provider, account, credential_source, plan, email, metrics }
UsageFetchDiagnostic { provider, kind, severity, message }
UsageFetchReport { outputs, diagnostics }
```

不建模 reset_credits / spend_control（本任务不做 Reset）。

### 提供者

| 显示名 | 凭证（只读） | 请求 |
| --- | --- | --- |
| Claude | `~/.claude/.credentials.json` 的 `claudeAiOauth.accessToken`；macOS 可只读 Keychain | `https://api.anthropic.com/api/oauth/usage` |
| Codex | Codex / OpenCode 本机 auth，不写 tokscale store、不 import current login | Codex usage endpoint（对照 tokscale `fetch_all_report`） |
| Grok Build | `GROK_HOME` 或 `~/.grok/auth.json` | tokscale 同款 subscriptions / task usage |
| Kimi | `KIMI_CODE_HOME` 或 `~/.kimi-code/credentials/kimi-code.json`，其次 `~/.kimi/credentials/` | tokscale 同款 usage URL；token 过期不 refresh |

测试用 `UsageEndpoints` 注入 base URL。生产默认值写在各模块常量。CI 禁止回落到公网。

### Health

对每个 `UsageOutput`，取 `metrics` 里最低的 `remaining_percent`：

- 无 metric → Unknown
- `< 10` → Critical
- `< 25` → Watch
- 否则 Ready

Summary 的 State / Capacity / Attention 用同一套阈值。Active 优先 `account.is_active`，否则第一行 Ready 账号；都没有则 `No active account`。

### TUI 构图

与 tokscale 同档：

- `width >= 132` 且 `height >= 20`：上半 Summary \| Selected；下半 Accounts
- `width >= 104`：纵向 Summary → Selected → Accounts
- 更窄：Accounts + 底部选中摘要

操作条：`r Refresh`、`m Show/Hide Emails`、`y Sync status`。不画 `a Add Codex`。

标题右上角：`{n} providers · {n} managed · {n} issues`。

### Overlay

```text
ActiveDialog::SyncStatus
```

渲染提取现有 `panels/usage.rs` 为 `panels/sync_status.rs`（或 overlay 调用同一套 summary/table/monitor 函数）。标题保持 `Usage / Sync` 仅出现在 overlay 内。

### 按键

| 键 | Usage 主区域 | overlay 打开时 |
| --- | --- | --- |
| `r` | 强制拉额度 + 刷新 sync 查询 | 关闭 dialog 后由主循环处理，或 overlay 内也刷新 sync |
| `R` | 只刷新 SQLite（含 overlay 数据） | 同左 |
| `m` | 切换邮箱可见 | 关闭或忽略，不改源选择 |
| `y` | 打开 Sync overlay | 保持打开 |
| `x` | 启动/取消本地 sync | 同左（dialog 不吞 `x`） |
| `s` | 源选择器 | 先关 overlay 再开 picker，或忽略 |
| Esc / `q` | 退出 dash（现行为） | 只关 overlay |

Help 与 footer 补上 `y` / `m`。

### 依赖与测试

- `reqwest`：`default-features = false`，features `["rustls-tls", "json", "http2"]`。用已有 tokio runtime。
- fetcher 单测：本机 `axum` 或 `tokio::net::TcpListener` 回放夹具。断言不写凭证文件。
- TUI 单测注入 `UsageFetchReport`，不启动 HTTP。
- `NO_COLOR` 源码守卫把新面板 / overlay 算进去。

### README

英文：local usage stays local；the Usage tab reads already-present CLI credentials and requests provider quota APIs.

中文：本地用量分析默认不上传；Usage 页会用本机已有 CLI 凭证读取订阅额度。

## Compatibility

- 无 schema migration。
- 无 web JSON 变化。
- 产品承诺从“永不调用云端用量 API”收窄为“默认不上传本地用量；Usage 页会读额度”。
- 回滚：删 `src/subscription`、还原 Usage 面板与 `reqwest` 即可。缓存文件可留在 `~/.llmusage/cache/`。

## Trade-offs

| 选择 | 原因 | 放弃的 |
| --- | --- | --- |
| 自写四家 fetcher，不 vendoring tokscale | 去掉写凭证、Add Codex、11 家提供者 | 不能一行不改地跟 tokscale |
| `R` 不拉额度 | 避免 30s 一次打到 429 | Usage 停留时额度会旧，靠 `r` |
| overlay 而不是删表 | dash 仍能看 rebuild-risk | 多一个 dialog |
| 额度不跟 TimeWindow | 云端窗口 ≠ 本地事件窗 | 不能按 7d 过滤账号 |
