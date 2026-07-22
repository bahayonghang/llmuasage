# 第一次报表

报表命令只读取本地 SQLite，不会触发同步。

## Daily 报表

```powershell
llmusage
llmusage daily
```

没有子命令时，`llmusage` 等价于 `llmusage daily`。默认 daily 报表显示所选时区下最近 7 个自然日（包含今天）。`--timezone local` 使用本机当前固定本地偏移；如果需要跨机器可复现的历史分组，请显式传入 `--timezone +08:00` 这类固定偏移。

常用筛选：

```powershell
llmusage daily --all
llmusage daily --since 2026-05-01 --until 20260518
llmusage daily --source codex
llmusage daily --project my-repo
llmusage daily --breakdown
llmusage daily --json
```

`--since` 和 `--until` 同时接受 `YYYYMMDD` 与 `YYYY-MM-DD`。daily、weekly、monthly 的人读表格使用共用的 coding-agent 视图：每个周期都有聚合 `All` 行，并在 `Agent` 列展示来源行。CLI JSON 使用 camelCase 字段；加上 `--by-agent` 后会在 JSON 中包含嵌套来源行。

`--no-cost` 会隐藏成本列与 JSON 成本字段，但不会改变 token 总量。

## Weekly 报表

```powershell
llmusage weekly
llmusage weekly --since 2026-05-04 --until 2026-05-10
```

weekly 使用该周周一的日期作为周期键，支持与 monthly 相同的报表筛选和 JSON 参数。

## Monthly 报表

```powershell
llmusage monthly --breakdown
```

Monthly 使用同一本地用量真源，支持日期范围、来源、JSON 和紧凑布局。

## 组合周期

```powershell
llmusage daily --sections weekly,monthly,session
llmusage monthly --sections daily,session --json
```

`--sections` 会把当前周期和请求的周期段组合到一份输出中，固定顺序为：当前命令周期在前，然后是 daily、weekly、monthly、session。JSON 对象保持同样顺序，并以当前命令周期的 `totals` 结尾。

## Session 报表

```powershell
llmusage session
llmusage session --id <session_id>
llmusage session --project my-repo
```

Session 报表优先使用来源 session metadata。旧数据库没有 metadata 时会退回稳定的源文件 key。

## 聚焦来源报表

```powershell
llmusage claude daily
llmusage codex monthly --json
llmusage opencode weekly --no-cost
llmusage antigravity session
```

`claude`、`codex`、`opencode`、`antigravity` 是 source host。每个都支持 `daily`、`weekly`、`monthly`、`session`，数据与 `<period> --source <source>` 相同。聚焦文本与 JSON 会移除 Agent 对比层；JSON 不含 `agent` 或 `agents` 字段。传入同值 `--source` 可以接受，冲突来源会被拒绝。`blocks` 有意不挂在 source host 下。

这是 llmusage 的均匀报表 surface，不表示各来源拥有相同的 ccusage 专属能力矩阵。

## Blocks 报表

```powershell
llmusage blocks --active
llmusage blocks --recent
llmusage blocks --token-limit max
```

`blocks` 生成 5 小时用量窗口和 burn-rate 预测。可用 `--session-length <hours>` 修改窗口。

## Statusline

```powershell
llmusage statusline
llmusage statusline --no-cache
```

`statusline` 输出一行适合状态栏使用的摘要。除非设置 `--no-cache`，否则可能在 `~/.llmusage/statusline-cache/` 写入很小的本地缓存。
