# 架构说明

本页说明当前 0.6.x 结构。设计决策见 [ADR](../adr/)，历史产品计划见 [PRD 历史档案](../prd/)。

## 运行时目录

运行时状态默认在 `~/.llmusage/`，可用 `--home <PATH>` 或 `LLMUSAGE_HOME` 覆盖。

- `llmusage.db` 保存 schema metadata、cursor、event、30 分钟 bucket、行为事实、项目元数据、source-file 诊断、集成状态、trigger 状态、价格元信息、worker lock 元信息和 run log。
- `bin/llmusage-hook.cmd` 与 `bin/llmusage-hook.sh` 是外部工具调用的本地 wrapper。
- `exports/` 保存静态 HTML 报告。
- `backups/` 保存卸载时用于回滚的集成配置备份。
- `pricing/` 保存 `doctor --refresh-pricing` 导入的本地价格快照。

## Source Registry

`SourceKind` 当前包含 Codex、Claude、OpenCode、Gemini；Gemini variant 同时覆盖 Google Antigravity，并保留 `gemini` 作为稳定持久化 id。Registry 是 parser 与 integration 的唯一 fan-out 点：

- `registered_parsers()` 驱动 `llmusage sync`。
- `registered_integrations()` 驱动 `init`、`doctor` 和 `uninstall` 类集成流程。

新增来源意味着新增 `SourceKind` variant，并注册一个 `SourceParser` 与一个 `Integration`。

## 同步流程

1. 工具专属 hook/plugin 触发 `llmusage hook-run`，或用户运行 `llmusage sync`。
2. 命令 bootstrap/migrate SQLite，并获取本地 `worker_lock`。
3. driver 按来源顺序执行注册 parser：Codex、Claude、OpenCode、Gemini。
4. 每个 parser 产出 `SyncShard`。
5. `SyncRunWriter::commit_shard` 执行 reset、event 写入、cursor 写入、raw archive 写入、行为事实写入和 source-file 标记。
6. Store 保存 per-source sync status 与 run-log 记录。

`SyncShard` 是 parser/writer 边界。Parser 不直接写 SQLite。

## 查询与 Dashboard 流程

报表命令、TUI、Web Dashboard 和 HTML export 都通过 query 层读取本地 SQLite。

`Dashboard::snapshot(&QueryFilter)` 是主要 Dashboard seam。`llmusage serve` 优先使用 `/api/dashboard`，用一个核心快照加载 overview、trend series、model/source/project/cost 排行、health 和 diagnostics。Activity、Tools、Optimize、Compare 是行为区块；当来源事实不可用或查询超时时，可以独立降级。

## 行为事实

0.6.x line 增加标准化行为表：

- `usage_turn`：Activity、Optimize、Compare 使用的 turn-level facts。
- `usage_tool_call`：Tools、Optimize、Compare 使用的 bounded tool/action facts。

隐私边界：行为事实不得保存完整 prompt、完整 assistant 文本或文件内容。`safe_preview` 只能是有界展示文本。

## Store façade

`Store` 是 paths、connections、worker locks、bootstrap、rebuild/reset 和 sync writer 创建的 façade。领域 store 通过 borrowed view 暴露，例如 `CursorStore`、`RunLog`、`SyncStatusStore`、`TriggerStore`、`SourceFileStore`。

## JobRegistry

`JobRegistry` 是供 library/web adapter 使用的进程内 sync job registry。它提供 start/get/cancel snapshots，但不是跨进程持久队列。持久恢复仍来自 SQLite usage 行、cursor、source-file diagnostics 和 run logs。

## Schema migrations

Schema migration 显式按版本推进。当前线包含：

- 原始 usage 表的 baseline migration，
- cache/cost/pricing 元数据，
- source-file state，
- raw archive opt-in，
- worker lock metadata，
- Gemini 注册（同时用于 Google Antigravity 兼容），
- `pricing_catalog_version`，
- 行为事实表，
- 对历史 `source_sync_status.stored_events` 漂移的兼容修复。

`schema_version` 本身不被当成完整安全证明；部署数据库发生漂移时，可以通过幂等兼容 migration 修复。

## 本地优先保证

- 不生成 device token。
- 不登录账号。
- 不建立上传队列。
- 不调用远端用量 API。
- 价格刷新读取用户提供的本地 JSON 文件。
- 浏览器 Dashboard 只绑定 `127.0.0.1`。
