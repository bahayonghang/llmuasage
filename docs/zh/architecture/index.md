# 架构说明

## 运行时目录

运行时目录固定在 `~/.llmusage/`：

- `llmusage.db`：保存 schema 元数据、cursor、event、bucket、项目元数据、source-file 诊断、集成状态、trigger 状态、价格元信息、worker lock 元信息和运行日志
- `bin/llmusage-hook.cmd` 与 `bin/llmusage-hook.sh`：外部工具调用的本地包装器
- `exports/`：静态 HTML 报告输出目录
- `backups/`：卸载时回滚配置用的备份

## 数据流

1. 外部工具触发本地 hook 或 plugin
2. `llmusage hook-run` 先记录 trigger，再尝试拿全局锁
3. worker 按注册顺序消费 Codex、Claude、OpenCode、Gemini 本地 parser
4. 每个 parser 发出 `SyncShard` 批次；writer 在同一提交协议里 reset 被替换文件、写 event、更新 cursor、标记 source-file 状态
5. 新事件带持久化 cache-aware 成本/pricing 元信息写入 `usage_event`；可选 raw archive 写入 `usage_event_raw`
6. 30 分钟 UTC 聚合（含成本/pricing rollup）写入 `usage_bucket_30m`
7. 报表命令、本地 Web UI、TUI、静态导出都从同一个 SQLite 读数据

## 本地优先边界

- 不生成 device token
- 不走登录流程
- 不做上传队列
- 不访问远端 API
- 不做 GitHub 在线公开性探测

项目展示名优先使用本地 git remote；本地路径默认只存 hash。价格刷新只读取用户提供的本地 JSON 文件；llmusage 不联网抓取价格。

## 报表层

`daily`、`monthly`、`session`、`blocks`、`statusline` 都是只读 SQLite 视图。它们复用 `usage_event` 作为报表真源，并把成本字段明确标为 `estimated_cost_usd`；从 0.5.1 起该值读取持久化 `cost_with_cache_usd`，不再查询时按 static-v1 重算。daily human 渲染默认是一张聚合的 ccusage 风格表，展示 `Input / Output / Cache Create / Cache Read / Total Tokens / Cost (USD)`；Source 维度通过过滤器和 breakdown 明细保留，JSON payload 仍保持聚合与 snake_case。session 报表优先使用 `session_id` metadata；旧数据库没有该字段时会使用稳定的源文件 fallback。`statusline` 可能在 `~/.llmusage/statusline-cache/` 写入很小的本地缓存；不会上传，也不会调用网络 API。

Web Dashboard 也是只读查询表面。live `llmusage serve` 优先调用 `/api/dashboard`，让 overview、day/week/month/all 趋势、模型/来源/项目/成本排行、health 和归档 diagnostics 都来自同一个 `Dashboard::snapshot(&QueryFilter)` 连接。旧的 `/api/overview`、`/api/trends`、`/api/models`、`/api/sources`、`/api/projects`、`/api/costs` 仍保持稳定，并接受同一套 filter query 参数用于兼容。


## 0.5.x 集成表面

ccr-ui 适配层保持薄包装：`Dashboard::snapshot`、`overview`、`trends_daily`、`home_overview`、`heatmap`、`logs`、`diagnostics` 与 `JobRegistry` 都读写同一个本地 SQLite 状态。CLI 报表、HTTP API、静态导出快照的 JSON 字段统一 snake_case。schema migration 显式推进到 v10；v10 记录 `pricing_catalog_version`，0.5.1 会把活动本地快照保存到 `~/.llmusage/pricing/<catalog-version>.json`，让后续 sync 继续使用同一本地 catalog。

`worker_lock` 串行化 CLI、hook、library worker。CLI/library sync 通过 `Store::acquire_worker_lock_with` 等待；高频 hook-run 保留旧的非阻塞路径，锁被占用时直接跳过。
