# 命令参考

## 报表命令

报表命令只读取 `~/.llmusage/llmusage.db`，不会自动触发 `sync`。如果本地数据库看起来过旧，请先运行 `llmusage sync`。

### `llmusage` / `llmusage daily`

按天展示 token 与估算成本。没有子命令时，`llmusage` 等价于 `llmusage daily`。

常用参数：

- `--since YYYYMMDD` / `--until YYYYMMDD`
- `--json`
- `--breakdown`
- `--instances` 按项目分组 daily 行
- `--project <label|hash|ref>`
- `--timezone UTC|local|+08:00`
- `--all` 显示完整 daily 历史；默认只显示今天
- `--compact`
- `--source codex|claude|opencode`

### `llmusage monthly`

按月聚合同一本地用量数据，支持 JSON、模型明细、日期范围、时区、紧凑表格和来源过滤。

### `llmusage session`

按来源会话聚合用量。使用 `--id <session_id>` 查看单个会话，使用 `--project` 按项目过滤。旧数据库没有 session metadata 时会使用稳定的源文件 fallback；运行 `llmusage sync --rebuild` 可从本地真源重新填充 session id。

### `llmusage blocks`

生成 5 小时用量窗口，用于 burn-rate 风格视图。

参数包括：

- `--active`
- `--recent`
- `--token-limit <number|max>`
- `--session-length <hours>`

### `llmusage statusline`

输出适合 hook/status bar 的单行摘要。有 stdin hook JSON 时会读取模型信息；默认在 `~/.llmusage/statusline-cache/` 写入轻量缓存，加 `--no-cache` 可关闭。

## 核心命令

### `llmusage init`

初始化本地运行时、创建 SQLite、生成 hook 包装器，并安装 Codex / Claude / OpenCode 三类集成。

### `llmusage sync`

顺序执行三类本地解析器，把增量结果写入 30 分钟 bucket。使用 `--rebuild` 会先清空可重建的 usage rows、bucket、project 和 cursor，再重新解析本地真源。

### `llmusage status`

输出人读摘要：数据库路径、bucket 数、最近同步、来源用量、集成状态、最近错误。

### `llmusage diagnostics`

输出机器可读 JSON，包括路径、集成状态、SQLite、cursor、来源统计、健康检查和最近运行记录。

### `llmusage doctor`

只读健康检查，覆盖包装器缺失、集成漂移、OpenCode DB 缺失、最近失败等问题。

## 本地界面命令

### `llmusage serve`

在 `127.0.0.1` 启动本地分析页和 JSON API，并默认用系统浏览器打开分析页。

![llmusage 本地 web 分析页概览](/screenshots/web-dashboard-overview.png)

### `llmusage tui`

打开终端摘要与运维面板。

### `llmusage export html`

导出静态目录：

- `index.html`
- `snapshot.json`
- `assets/*`

## 卸载

### `llmusage uninstall`

恢复被改过的集成配置。只有加 `--purge` 时才会删除 `~/.llmusage/`。
