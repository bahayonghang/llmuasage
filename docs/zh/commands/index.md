# 命令参考

## 报表命令

报表命令只读取 `~/.llmusage/llmusage.db`，不会自动触发 `sync`。如果本地数据库看起来过旧，请先运行 `llmusage sync`。

### `llmusage` / `llmusage daily`

按天展示 token 与估算成本；估算成本读取持久化 cache-aware `cost_with_cache_usd` 列。没有子命令时，`llmusage` 等价于 `llmusage daily`。

常用参数：

- `--since YYYYMMDD` / `--until YYYYMMDD`
- `--json`
- `--breakdown`
- `--instances` 按项目分组 daily 行
- `--project <label|hash|ref>`
- `--timezone UTC|local|+08:00`
- `--all` 显示完整 daily 历史；默认只显示今天
- `--compact`
- `--source codex|claude|opencode|gemini`

### `llmusage monthly`

按月聚合同一本地用量数据，支持 JSON、模型明细、日期范围、时区、紧凑表格和来源过滤。

### `llmusage session`

按来源会话聚合用量。使用 `--id <session_id>` 查看单个会话，使用 `--project` 按项目过滤。旧数据库没有 session metadata 时会使用稳定的源文件 fallback；只有本地真源文件仍在时，才建议运行 `llmusage sync --rebuild` 重新填充 session id。

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

初始化本地运行时、创建 SQLite、生成 hook 包装器，并安装 Codex / Claude / OpenCode / Gemini 集成。

### `llmusage sync`

顺序执行 Codex、Claude、OpenCode、Gemini 本地解析器，把增量结果写入 30 分钟 bucket，包括持久化 cost/pricing rollup。可用 `--source codex|claude|opencode|gemini` 限定来源；使用 `--rebuild` 会先清空可重建的 usage rows、bucket、project 和 cursor，再重新解析本地真源。删除前会对文件型来源做预检：如果已导入事件依赖的源文件现在缺失，默认拒绝执行。普通 `llmusage sync` 在这种状态下是安全的，只会把源文件标记为 missing 供 diagnostics 使用，不会删除 usage history。只有明确接受清掉不可重建历史时，才把 `--allow-lossy-rebuild` 与 `--rebuild` 一起传入。默认进度写入 stderr，stdout 保留最终摘要；`--json-events` 则在 stdout 输出 NDJSON 生命周期/进度事件。

### `llmusage status`

输出人读摘要：数据库路径、bucket 数、最近同步、来源用量、集成状态、最近错误。

### `llmusage diagnostics`

输出机器可读 JSON，包括路径、集成状态、SQLite、cursor、来源统计、source-file 归档诊断、健康检查和最近运行记录。来源归档行包含 `missing_file_count`、`protected_event_count` 与 `lossy_rebuild_risk`，用于区分“原始源文件缺失”和“已导入 usage 丢失”。`--forget-file <PATH>` 可把源文件标记为用户主动忽略；同一路径出现在多个来源时需要配合 `--source`。

### `llmusage doctor`

默认执行只读健康检查，覆盖包装器缺失、集成漂移、本地真源存在性、最近失败等问题。`--refresh-pricing <file>` 是唯一写入模式：导入本地价格 JSON 快照，保存为 `~/.llmusage/pricing/<catalog-version>.json`，重算 event 与 bucket 成本，并记录 `pricing_catalog_version`。

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
