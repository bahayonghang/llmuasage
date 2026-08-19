# Dash Usage 对齐 tokscale 订阅额度页

## Goal

让 `llmusage dash` 的 Usage 页达到 tokscale Usage 的可读性：先看到账号额度是否够用，再看到各账号剩余与重置时间。

## User Value

用户在 dash 里就能判断本周额度、哪个账号可切、何时重置，不必再开 tokscale。本地 Source Sync 仍可在 overlay 里核对。

## Background

对照来源：用户 2026-08-19 两张截图、`ref/repo/tokscale` 的 `commands/usage` 与 `tui/ui/usage.rs`、`src/tui/panels/usage.rs`、`Dashboard::sync_command_center`。完整对照见 `research/tokscale-usage-gap.md`。

当前 `Panel::Trends` 显示名为 Usage，加载 `SyncCommandCenterPayload`：Rebuild risk、Source Sync 表、Platform Monitor。`x` 启动本地 sync。`r` / `R` 重载 SQLite 面板。

tokscale Usage 是订阅额度看板：操作条、Usage Summary、Selected Account、Accounts 表（`#` / Provider / Account / Plan / Auth / Health / Limit / Reset）。数据来自本机 CLI 凭证加云端用量 API。进入 tab 自动拉取，失败进 Diagnostics。Health 由剩余额度派生：`<10%` Critical，`<25%` Watch，否则 Ready。

`README.md` / `README.zh-CN.md` 写明不上传、不登录、不调用云端用量 API。根 `Cargo.toml` 没有出站 HTTP 客户端。web `/api/dashboard` 的 `sync_command_center` 与 TUI 共用查询，本任务不改该 JSON。

## Requirements

- R1. Usage 主区域换成 tokscale 构图：操作条、Usage Summary、Selected Account、Accounts 表。主区域缓冲区不含 `Usage / Sync`、`Source Sync`、`Platform Monitor`。
- R2. Accounts 宽表列：`#`、Provider、Account、Plan、Auth、Health、Limit、Reset。窄屏改为双行 Account / Status。`j/k/Pg` 选择账号；选中行驱动右侧/下方详情。
- R3. Summary 展示 State、Active、Capacity、Fallback、Next Reset、Action，以及 Diagnostics、Attention、Providers。高度不够时按 tokscale 顺序裁行。
- R4. 选中账号展示 Status、Email、Credential、Limits 进度条、Snapshot。默认隐藏完整邮箱，显示 `[hidden email]`。`m` 切换显示/隐藏。
- R5. MVP 提供者：Grok Build、Kimi、Claude、Codex。只对 `has_credentials()` 为真的提供者发请求。无凭证的不占表行。Amp、Copilot、MiniMax、Warp、Sakana、Z.ai 不做。
- R6. 进入 Usage 或按 `r` 拉取额度。进入时若缓存未过期（5 分钟）则用缓存。`r` 绕过缓存强制拉取。`R` 自动刷新只重载 SQLite 面板，不轮询额度。打开 Overview 或其他 tab 不发用量请求。
- R7. 拉取失败写入 Diagnostics，不丢已成功的其他提供者。无任何凭证时给空态，不伪装成已同步。
- R8. 凭证只读。不 refresh、不写回 Claude / Kimi / Grok / Codex 凭证文件。过期或 401/403 记诊断。不实现 Add Codex、reset credit、账号 Use/Remove。
- R9. Source Sync 与 Platform Monitor 迁到与 Help / Source picker 同类的 overlay。`y` 打开。overlay 内仍能看到 rebuild-risk、按源计数和 monitor-only 探测。`x` 仍启动/取消本地 sync。`s` 仍是源过滤。
- R10. `NO_COLOR` / `LLMUSAGE_NO_COLOR` / ANSI16 保持 `tui-presentation-contracts.md`。面板和 overlay 不写 `Color::*`。交互文案保持英文。额度百分比与 reset 文案不用 `stat_compact`；overlay 里的 sync 计数仍用精确分组格式。
- R11. 不改 `usage_event` / bucket 主键。不改 web `sync_command_center` 字段集。不新增 CLI `usage` 子命令。
- R12. README 与中文 README 改为：本地用量分析默认不上传；Usage 页会用本机已有 CLI 凭证读取订阅额度。
- R13. TestBackend 覆盖：新标题与表列、空态、诊断行、默认隐藏邮箱、`m` 切换、宽/窄构图、overlay 仍含 Source Sync、NoColor。fetcher 单测走本地假服务器，CI 不访问公网。

## Acceptance Criteria

- [ ] AC1. `llmusage dash` 打开 Usage，主区域是额度 Summary + 选中账号 + Accounts 表；缓冲区不含 `Source Sync`、`Platform Monitor`、`Usage / Sync`。
- [ ] AC2. 本机有凭证的 Grok Build / Kimi / Claude / Codex 能显示 remaining 与 reset；其中一路失败时 Diagnostics 有对应行，其他成功行仍在表里。
- [ ] AC3. 默认不展示完整邮箱。按 `m` 后详情里出现真实邮箱（若提供者返回了邮箱）。
- [ ] AC4. 按 `y` 打开 overlay，缓冲区含 `Source Sync` 与 `Platform Monitor`。`x` 仍启动或取消本地 sync。
- [ ] AC5. 进入 Usage 会拉取或命中 5 分钟缓存。`r` 强制重拉。`R` 打开后停留在 Usage 不会周期性请求云端用量 API。
- [ ] AC6. 凭证文件在一次失败拉取前后字节不变。
- [ ] AC7. `NO_COLOR=1` 下无前景色、无修饰。
- [ ] AC8. web `/api/dashboard` 的 `sync_command_center` 字段集不变。
- [ ] AC9. README 与 `README.zh-CN.md` 不再写“不调用云端用量 API”的无例外承诺。
- [ ] AC10. `cargo fmt --check`、严格 Clippy、相关 TUI / subscription 测试通过。CI 测试不访问公网。

## Out of Scope

- 改 `serve` / ccr-ui 的同步指挥中心页面。
- Amp、Copilot、MiniMax、MiniMax Token Plan、Warp/Oz、Sakana、Z.ai。
- Add Codex、reset credit、账号切换/删除、写回第三方凭证。
- CLI `llmusage usage` 子命令。
- 像素级复制 tokscale 主题色。
- 把本地 sync 计数映射成假的 Limit / Reset。
- Usage 主表跟随 TimeWindow 或 source 过滤（额度是账号窗口，不是事件窗口）。

## Decisions

| 决策 | 选择 | 日期 |
| --- | --- | --- |
| 产品形态 | 真实订阅额度页 | 2026-08-19 |
| Source Sync | TUI overlay（`y`），不新增 tab，不塞进 Stats | 2026-08-19 |
| 第三方凭证 | 只读；不 refresh、不写回 | 2026-08-19 |
| 拉取时机 | 进入 Usage 或 `r`；5 分钟缓存；`R` 不轮询额度 | 2026-08-19 |
| MVP 提供者 | Grok Build、Kimi、Claude、Codex | 2026-08-19 |
| Amp | 不做 | 2026-08-19 |
| README | 写明 Usage 会用本机 CLI 凭证读订阅额度 | 2026-08-19 |
