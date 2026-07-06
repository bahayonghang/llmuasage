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
llmusage daily --since 20260501 --until 20260518
llmusage daily --source codex
llmusage daily --project my-repo
llmusage daily --breakdown
llmusage daily --json
```

人读表格使用聚合 ccusage 风格列：`Date`、`Models`、`Input`、`Output`、`Cache Create`、`Cache Read`、`Total Tokens`、`Cost (USD)`。

## Monthly 报表

```powershell
llmusage monthly --breakdown
```

Monthly 使用同一本地用量真源，支持日期范围、来源、JSON 和紧凑布局。

## Session 报表

```powershell
llmusage session
llmusage session --id <session_id>
llmusage session --project my-repo
```

Session 报表优先使用来源 session metadata。旧数据库没有 metadata 时会退回稳定的源文件 key。

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
