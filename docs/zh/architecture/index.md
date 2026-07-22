# 架构说明

本页说明当前 0.6.x 结构。设计决策见 [ADR](../adr/)，历史产品计划见 [PRD 历史档案](../prd/)。

## 运行时目录

运行时状态默认在 `~/.llmusage/`，可用 `--home <PATH>` 或 `LLMUSAGE_HOME` 覆盖。

- `llmusage.db` 保存 schema metadata、cursor、event、30 分钟 bucket、行为事实、项目元数据、source-file 诊断、集成状态、trigger 状态、价格元信息、worker lock 元信息和 run log。
- `bin/llmusage-hook.cmd` 与 `bin/llmusage-hook.sh` 是外部工具调用的本地 wrapper。
- `exports/` 保存静态 HTML 报告。
- `backups/` 保存卸载时用于回滚的集成配置备份。
- `pricing/` 保存由 `catalog` 或 `doctor --refresh-pricing` 激活的内容寻址 base、overlay 和 effective 价格目录。

## Source Registry

`SourceKind` 当前包含 Codex、Claude、OpenCode、Antigravity。`antigravity` 是稳定 CLI/API/SQLite 来源 id；`gemini-*` 字符串仍只是模型 id。

`SourceDescriptor` 是来源能力注册表，声明每个来源的稳定 id、别名、激活方式（`hook`、`plugin`、`passive` 或 `hybrid`）、parser/integration 能力、token 质量标签和本地隐私边界。Registry 是 parser、integration 与 descriptor 的唯一 fan-out 点：

- `registered_parsers()` 驱动 `llmusage sync`。
- `registered_integrations()` 驱动 `init`、`doctor` 和 `uninstall` 类集成流程。
- `registered_source_descriptors()` 驱动 capability/status 语义，并用测试防止 parser/integration 漂移。

新增来源意味着新增 `SourceKind` variant 和 descriptor。只有 descriptor 的能力声明与测试证据支持时，才新增 parser 或 integration。Passive reader 写入 usage 行之前还必须具备真实本地样本、fixture 覆盖、sync-twice 幂等、cursor/rebuild 行为、token 质量声明和隐私审查。

`PlatformMonitorDescriptor` 是更宽的监控目录，可以描述来自 tokscale 风格证据的 parserless 候选，包括 Gemini CLI、Cursor、Copilot、Zed、Kiro、Goose、Grok、Kimi/Qwen、Roo/Kilo/Cline、Codebuff、Crush、Warp/Oz、Amp、Hermes 和 Trae。Monitor descriptor 可以在 `source-status` 与 `dash` 中展示 detected/unavailable 根目录、parser 支持状态、隐私类别、token 质量和下一步动作，但它们不会作为 `SourceKind` 持久化，也不能写入 usage 行。

## 同步流程

1. 工具专属 hook/plugin 触发 `llmusage hook-run`，或用户运行 `llmusage sync`。
2. 命令 bootstrap/migrate SQLite，并获取本地 `worker_lock`。
3. 手动 sync 按来源顺序执行注册 parser：Codex、Claude、OpenCode。Antigravity 在有验证过的 transcript schema 前仅作为 hook/integration 来源。hook-run sync 会限制到触发来源，避免一个 hook 导入所有 parser-backed 来源。
4. 每个 parser 产出 `SyncShard`。
5. `SyncRunWriter::commit_shard` 执行 reset、event 写入、cursor 写入、raw archive 写入、行为事实写入和 source-file 标记。
6. Store 保存 per-source sync status 与 run-log 记录。

Codex `notify` 是 singleton integration。llmusage 安装时会备份不同的原 notify，并在自身 hook 处理后 best-effort 链式启动；递归/自身命令会被跳过，链式命令失败不会阻塞 hook 成功。

`SyncShard` 是 parser/writer 边界。Parser 不直接写 SQLite。

重复 sync 工作通过每个来源自己的 cursor 避免。Codex 和 Claude 会在重解析前比较文件大小、mtime、头部 fingerprint、尾部签名和 offset；OpenCode 会比较 DB 身份和 message 高水位 cursor。Sync stats 会把未变化工作显示为 skipped，把变化 artifact 显示为 parsed，把本次新增写入显示为 committed，把数据库持久总量显示为 stored events。

## 查询与 Dashboard 流程

报表命令、TUI、Web Dashboard 和 HTML export 都通过 query 层读取本地 SQLite。

`Dashboard::snapshot(&QueryFilter)` 是主要 Dashboard seam。`llmusage serve` 优先使用 `/api/dashboard`，用一个核心快照加载 overview、trend series、model/source/project/cost 排行、health、diagnostics 和默认 Explorer payload。Activity、Tools、Optimize、Explorer、Compare 是行为/查询区块；当来源事实不可用或查询超时时，可以独立降级。

自定义 Cost Explorer 查询使用 `Dashboard::explorer(&ExplorerQuery)` 和 `/api/explorer` endpoint。Explorer 叠加在固定 Dashboard snapshot 之上：它支持时间粒度、指标、分组、Top N/Other、session/tool/token 过滤，但仍返回后端聚合后的 rows 和 series，不让浏览器透视原始事件。查询层会根据所选指标和维度选择 event、turn 或 tool-attribution 策略，每个 payload 都携带 `normalized`、`no_data`、`degraded` 或 `unsupported` 等 support metadata。

## 价格目录流程

内置 `pricing/static-v2.json` 是默认 base，统一保存稳定模型 id、来源展开、exact/family matcher、默认/阈值费率和上下文窗口。Parser 写入 SQLite 的原始模型字符串不变。

可选用户层是通过 `catalog apply` 显式激活的 v2 `overlay`。合并顺序固定：校验 base，执行严格 `remove_models`，按稳定 id 完整替换或追加定义，最后校验 effective catalog。不做字段级深合并。Exact matcher 优先于 family matcher，同模式下更长 matcher 优先。

`~/.llmusage/pricing/` 下的目录文件使用 SHA-256 文件名：

- `base-<sha256>.json`：固定的 base 副本；
- `overlays/overlay-<sha256>.json`：用户 overlay；
- `effective-<sha256>.json`：规范化合并后的目录。

SQLite meta 记录 active、base、overlay 的身份和文件。已选择文件缺失、被修改或无效时会 fail closed，不会回退内置目录。apply 先写入并校验文件，再逐条重算 `usage_event`、协调 30 分钟 bucket 定价，最后在一个事务中切换全部 catalog metadata。`doctor --refresh-pricing` 复用同一激活服务导入完整 base snapshot。

阈值费率按 event 的 input + cache-read + cache-creation token 选择。Bucket 和报表只汇总已落库 event 成本。Context-pressure 查询加载当前活动目录，因此配置模型会使用其配置的上下文窗口。

## 行为事实

0.6.x line 增加标准化行为表：

- `usage_turn`：Activity、Optimize、Compare 和 turn-backed Explorer 查询使用的 turn-level facts。
- `usage_tool_call`：Tools、Optimize、Compare 和 tool-attribution Explorer 查询使用的 bounded tool/action facts。

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
- Antigravity 来源注册，
- active/base/overlay 价格目录元数据，
- 行为事实表，
- v13 `gemini` → `antigravity` 来源 id cutover，
- 对历史 `source_sync_status.stored_events` 漂移的兼容修复。

`schema_version` 本身不被当成完整安全证明；部署数据库发生漂移时，可以通过幂等兼容 migration 修复。

## 本地优先保证

- 不生成 device token。
- 不登录账号。
- 不建立上传队列。
- 不调用远端用量 API。
- 价格目录激活只读取用户提供的本地 JSON 文件，不会联网拉取价格。
- 浏览器 Dashboard 默认绑定 `127.0.0.1`；`serve --public` 会显式改为监听 `0.0.0.0`，但不会添加认证或 TLS。
