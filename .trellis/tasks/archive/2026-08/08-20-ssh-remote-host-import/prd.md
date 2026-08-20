# SSH 远端主机导入与同步

## Goal

让用户在一台机器上看到多台机器（含通过 SSH 访问的远程服务器）的 AI CLI 用量，并且报表能够区分每条用量属于哪台主机。

采用方案 C：远端执行 llmusage 完成解析与规范化，把规范化结果经 SSH 传回本地，本地通过既有 `SyncRunWriter::commit_shard` 协议落库。

## Background

### 为什么解析必须发生在远端

`ProjectResolver::resolve_project_info`（`src/domain/project.rs:30-60`）从会话记录的 `cwd` 逐级上溯查找 `.git`，再读 `.git/config` 取 remote URL 得到 `project_ref`；Codex 的 `cwd` 来自会话文件字段（`src/parsers/codex.rs:130-134`）。

把远端会话文件拷回本地解析会产生两种结果：本地无同名路径时 `project` 为 NULL，归属丢失；本地恰好存在同名路径时归属指向本地另一个仓库，归属错误。因此解析必须在拥有会话文件与 git 工作树的机器上完成。

远端只回传规范化字段（token、model、project 维度），不回传 prompt 正文，与现有就地只读的隐私姿态一致。

### 现状事实（已通过代码确认）

**解析层绑定本地文件系统。** `src/parsers/source_files.rs` 各 source 根目录来自 `resolve_home_dir()` 加各自环境变量（`CODEX_HOME`、`KIMI_CODE_HOME`、`GROK_HOME`、`PI_AGENT_DIR`、`GEMINI_CLI_HOME`、`DSH_HOME`）；Claude 无环境变量覆盖，硬编码 `~/.claude/projects`（`source_files.rs:62-67`）。枚举用 `walkdir`，读取用 `std::fs` 加 offset 增量，opencode / antigravity / zcode 直接打开 SQLite 文件。无 reader / VFS 抽象层。

**schema 无 host 维度。**

| 表 | 当前主键 | 跨主机后果 |
|---|---|---|
| `usage_event` | `event_key`（`migrations.rs:275`） | 见 event_key 一节 |
| `usage_bucket_30m` | (source, provider_label, model, hour_start, project_hash)（`migrations.rs:693`） | 多主机静默合并同一行 |
| `source_file` | (source, file_path)（`migrations.rs:503`） | 路径相同即主键冲突 |
| `source_cursor` | (source, cursor_key)，`cursor_key` 为文件路径字符串（`parsers/file_state.rs:114`） | 路径相同即主键冲突，cursor 互相覆盖 |
| `source_sync_status` | source（`migrations.rs:354`） | 每 source 一行，无法表达按主机的同步状态 |

`/root/.codex/sessions/...`、`/home/ubuntu/.claude/projects/...` 在多台服务器上重复出现属常态。

**event_key 构造按 source 不同，均不含主机标识。** 事件写入使用 `INSERT OR IGNORE`（`store/sync_writer.rs:316`），键相同即静默丢弃后来者。

| source | event_key 构造 | 跨主机性质 |
|---|---|---|
| claude | `claude:logical:hash(message_id\0request_id)`；无 message_id 时回退 `claude:{path_hash}:{file_fingerprint}:{offset}`（`parsers/claude.rs:685-702`） | 主键用 API 下发 id，跨主机唯一；回退键由路径派生 |
| codex | `codex:logical:hash(timestamp\0model\0token 增量)`（`parsers/codex.rs:189-198`） | 不含 session、路径、主机；元组相同即少计 |
| kimi_code | `kimi_code:hash(path_hash\0start_offset\0time_ms\0model\0tokens)`（`parsers/kimi_code.rs:413-423`） | 由路径与文件内偏移派生 |
| pi | `pi:hash(path_hash\0start_offset\0event_at\0model\0tokens)`（`parsers/pi.rs:420-430`） | 由路径与文件内偏移派生 |
| deepseek_harness | `deepseek_harness:hash(identity\0time_ms\0provider\0model\0tokens)`，identity 取上游 message id，缺失时回退 `sid:{session_id}`（`parsers/dsh.rs:580-599`） | 上游 id 存在时跨主机唯一；回退时由 session id 派生 |
| grok | `grok:{session_id}:{sidecar}`（构造于 `parsers/grok.rs:744-757`，测试断言 `parsers/grok.rs:930`） | 由 session 目录名派生 |
| antigravity | `antigravity:{path_hash}::hash(response_id)`（`parsers/antigravity.rs:479`） | 含 path_hash；该 source 已为 historical_only |
| opencode | `opencode:{row.id}`，row.id 取 OpenCode message 表主键（`parsers/opencode.rs:461`） | 由上游本地 DB 生成 |
| zcode | `zcode:hash(row.id)`（`parsers/zcode.rs:632`） | 由上游本地 DB 生成 |

多数 source 的键由 `path_hash` 或文件内偏移派生，跨主机不具备唯一性。

**派生键与 event_key 的耦合。** 采用全局统一 host 前缀后，需要同步重写的键列如下：

| 位置 | 构造 | 说明 |
|---|---|---|
| `usage_event.event_key` | 主键（`migrations.rs:275`） | 前缀变更的源头 |
| `usage_event_raw.event_key` | 主键（`migrations.rs:535`） | 与 usage_event 一一对应 |
| `usage_turn.turn_key` | 主键，`format!("turn:{}", event.event_key)`（`domain/models.rs:488`、`parsers/behavior.rs:52`） | 内嵌 event_key |
| `usage_tool_call.tool_call_key` | 主键，`format!("tool:{source}:{event_key}:{sequence}")`（`parsers/behavior.rs:46-51`）；opencode 另有 `tool:opencode:{key_seed}`（`parsers/opencode.rs:646`） | 内嵌 event_key |
| `usage_tool_call.turn_key` | `format!("turn:{}", event.event_key)`（`parsers/behavior.rs:52`） | 内嵌 event_key |
| `usage_tool_call.event_key` | 直接引用（`migrations.rs:743`） | 直接引用 |

读取层依赖 `substr(t.turn_key, 6)` 从 turn_key 还原 event_key（`query/explorer.rs:695`、`query/explorer.rs:730`），并有表达式索引 `idx_usage_turn_event_key_expr ON usage_turn(substr(turn_key, 6))`（`migrations.rs:764-765`）。只要 `turn:` 前缀长度不变，该 join 与索引在重写后仍成立。

`Store::reset_for_source` 用 `usage_event_raw.event_key LIKE '{source}:%'` 定位待删 raw 行（`store/schema.rs:262-270`）。加入 host 前缀后该模式失效，必须改写；且该函数在同一事务内先删 `usage_event` 再删 raw，无法改为 join，需要调整删除顺序或改用子查询。

**source_file 状态机与 rebuild 守卫和间歇可达的远端冲突。** driver 在每个 source 解析结束后把本轮未见到的 `live` 行扫为 `missing`（`parsers/driver.rs:121-128`，ADR 0006）。远端离线时其整份清单被判 `missing`，随后 `sync --rebuild` 被 `lossy_rebuild_risks` 拒绝（`commands/sync.rs:768-805`），自动 token-accounting 修复同样拒绝执行（`commands/sync.rs:772-780`）。

**读取层无 host 概念。** `QueryFilter` 仅有 source / model / since / until / project_hash / timezone（`query/filter.rs:29-41`）。既有模式为 `--source` 过滤（`commands/report_args.rs:62-64`）加可选的每 source 行（`commands/report_args.rs:106`、`query/reports.rs:597`）。相关读取面：`src/query/mod.rs` 285 KB、`query/reports.rs` 108 KB、`query/explorer.rs` 75 KB、`web/mod.rs` 6181 行。

**无用户配置文件。** llmusage 自身没有 config 文件，`toml_edit` 仅用于读取 Codex 的 `config.toml`。远端定义需要新的持久化位置。

**成本在写入时计算。** `begin_sync_run` 加载活动 pricing catalog（`store/sync_writer.rs:97-105`），`write_event_batch_tx` 内调用 `pricing::compute_cost_with` 写入 cost 列（`store/sync_writer.rs:331-338`）。因此远端只需回传 token 与 model，cost 由本地 pricing catalog 统一计算，跨主机 pricing 版本差异不进入数据。

**安装机制。** 现有安装只有 `llmusage update` 走 `cargo install --git`（`commands/update.rs:60-68`），仓库不分发预编译二进制。

**可复用接缝。** `SyncShard { source, reset_path_hashes, events, cursors, seen_file_paths, raw_records, turns, tool_calls }`（`store/mod.rs:742`）加 `SyncRunWriter::commit_shard`（ADR 0002）；`Store::begin_sync_run()` 为 public（`store/sync_writer.rs:77`）。`UsageEvent`、`UsageTurn`、`UsageToolCall`、`ProjectInfo`、`SessionInfo`、`FileCursor` 均已 derive `Serialize`/`Deserialize`；`SyncShard` 与 `RawRecord` 目前只 derive `Debug`（`store/mod.rs:741`、`store/mod.rs:804`）。`sync --json-events` 已建立 NDJSON stdout 输出先例（`parsers/mod.rs:50`）。

**写围栏约束。** `.trellis/spec/llmusage/backend/write-fencing-contracts.md` 规定 `commit_shard` 是唯一的 shard 写入协议，且 sync 必须先取锁、派生 fenced Store，再经该 Store 贯穿 run-log、driver、shard writer 与 status 写入；不得为 cursor、status、run-log、catalog 增加新的控制面例外。远端导入必须走同一条路径。

### 与既有决策的冲突（需要显式处理）

- ADR 0011 确立 passive parsing 为唯一导入机制
- `docs/prd/llmusage-integration-prd-v1.1.md:774`：不引入云端上传 / 远程聚合
- `docs/safety/index.md:29`：无上传队列、无远端用量 API 调用
- `docs/index.md:21`：No hooks, plugins, login, sync service, or remote usage API
- 现有文档把 SSH 定位为访问通道：`docs/reference/cli.md:288` 写 loopback 经 SSH 隧道；`docs/dashboard/index.md` 的「Remote or SSH access」节（`:24`）目前描述的是 `serve --public`
- `.trellis/spec/llmusage/backend/token-accounting-contracts.md` 记载 Codex 采用 source-scoped 逻辑身份；加入 host 前缀后变为 host 加 source 作用域

用户自有主机、用户主动触发的拉取不构成云服务，解析机制仍是 passive parsing，变化的只是解析发生地。仍需新 ADR 与文档同步修改。

## Key Decisions

**D1：MVP 一次交付四块。** host 维度 schema 迁移；SSH 传输与远端 shard 输出；读取层 host 维度（CLI 加 dashboard）；远端生命周期语义。

**D2：event_key 采用全局统一 host 前缀，含本地主机。** 本地 host_id 固定为 `local`，其事件键同样加前缀。存量行需一次性重写六个键列，并改写 `Store::reset_for_source` 的 raw 删除条件。

已向用户说明该方案相对仅远端加前缀风险更高：存量 `event_key` 全部变更，涉及四张表的主键与引用列，需要一次性全量重写；用户确认采用全局统一前缀。因此验收标准覆盖迁移前后的总量、cost 与父子关联不变。

前缀施加位置：由 `commit_shard` 按 shard 的 host_id 集中施加，parser 保持 host 无关；派生键在同一处按已加前缀的 event_key 生成，不在九个 parser 中重复实现。

该决策改变 token-accounting-contracts 记载的 Codex 键作用域，且使同一份会话产物在两台已注册主机上被解析时计为两条，需在文档中说明。

**D3：远端要求预装 llmusage，`remote add` 探测并握手。** `host` 表存 `command` 字段（默认 `llmusage`），允许绝对路径或包装命令，用于绕过非登录 shell 的 PATH 限制。探测执行 `ssh <target> <command> --version`，再执行隐藏子命令 `remote handshake` 读取 shard 协议版本与 schema 版本。拒绝注册的条件是探测失败或 **shard 协议版本** 与本地不等；schema 版本只写入错误信息作诊断，不作为拒绝条件。不做二进制推送，不在远端自动 `cargo install`。

**D4：host 是与 source 平行的独立维度。** `--host` 过滤加可选每主机行，与既有 per-source 行同构；dashboard 新增独立主机分组，不改造现有 source 分组。

**D5：不可达远端跳过并告警。** `llmusage sync` 自动包含已注册远端，单台远端不可达不得中断本地同步，也不得让整次 sync 的退出码变为失败。

## Requirements

### R1 host 注册与持久化

- R1.1 新增 `host` 表：`host_id`、`label`、`transport`（`local` / `ssh`）、`ssh_target`、`command`、`added_at`、`last_contacted_at`、`last_error`、`import_watermark`。
- R1.2 本地主机以固定 `host_id = 'local'` 注册，由迁移写入。
- R1.3 新增 `llmusage remote add <label> <ssh-target> [--command <path>]`、`remote list`、`remote remove <label>`。
- R1.4 `remote add` 执行探测与 shard 协议握手；探测失败或 shard 协议版本与本地不等时拒绝注册，不写入 `host` 行，并输出两端协议版本、远端 schema 版本与可执行的修复指引。
- R1.5 `remote remove` 默认保留该主机已导入的用量行，并提示清除方式。
- R1.6 `host_id` 取自 `label` 的规范化形式；与已有 `host_id` 冲突、或等于任一 `SourceKind::as_str()` 时拒绝注册，不自动改名。

### R2 host 维度 schema 与 event_key 前缀

- R2.1 `usage_event`、`usage_turn`、`usage_tool_call`、`usage_bucket_30m`、`source_file`、`source_cursor`、`source_sync_status` 增加 `host_id`。`usage_turn` 与 `usage_tool_call` 需要该列才能让 behavior 与 Activity 视图按主机过滤。
- R2.2 主键调整为 `usage_bucket_30m`(host_id, source, provider_label, model, hour_start, project_hash)、`source_file`(host_id, source, file_path)、`source_cursor`(host_id, source, cursor_key)、`source_sync_status`(host_id, source)。
- R2.3 存量行回填 `host_id = 'local'`。
- R2.4 `event_key` 统一为 `{host_id}:{原键}`，含本地主机。
- R2.5 迁移同步重写 `usage_event.event_key`、`usage_event_raw.event_key`、`usage_turn.turn_key`、`usage_tool_call.tool_call_key`、`usage_tool_call.turn_key`、`usage_tool_call.event_key`。
- R2.6 `Store::reset_for_source` 的 raw 删除条件改为按 host 加 source 限定。
- R2.7 前缀由 `commit_shard` 集中施加；parser 输出保持不含 host。

### R3 远端解析、传输与落库

- R3.1 远端侧新增机器可读 shard 输出：`llmusage sync --emit-shards`，NDJSON 到 stdout。第一条成功反序列化的记录必须是含 llmusage 版本、schema 版本与 `shard_protocol` 的 header。该命令不打开、不写入用户 `~/.llmusage` 库，不取用户库上的 worker lock，不推进用户库 cursor。
- R3.2 `SyncShard`、`RawRecord` 增加 `Serialize` / `Deserialize`。
- R3.3 本地经 `ssh` 子进程调用远端命令，逐行反序列化，经 `commit_shard` 在 fenced Store 下落库，并写入该 host 的 `source_sync_status`。
- R3.4 传输内容仅规范化字段；`raw_records` 不跨机器传输。
- R3.5 增量边界由本地权威：本地保存每台远端的 `import_watermark`，请求远端时下发 cutoff（复用既有 `recent_cutoff` 机制），并保留安全重叠窗口，重复事件由 `event_key` dedupe 吸收。首次导入不带 cutoff。
- R3.6 本地导入器对远端 stdout 中非 JSON 行与未知 `kind` 计数后跳过，不得使整批导入失败。跳过行数由本地解析器计入告警，不依赖远端 trailer。

### R4 读取层 host 维度

- R4.1 `QueryFilter` 与 CLI 的 `ReportFilter` 都增加 `host_id`。dashboard / explorer 走 `QueryFilter`；daily / weekly / monthly / session / blocks / focused 走 `ReportFilter`。
- R4.2 daily / weekly / monthly / session / blocks 与 focused 命令支持 `--host <LABEL>` 过滤；`ReportCommonArgs::to_filter` 把 label 解析为 `host_id`。
- R4.3 提供可选每主机行，与既有 per-source 行同构，并进入 CLI JSON 报表。
- R4.4 dashboard 增加独立主机分组，不改造现有 source 分组。
- R4.5 `source-status` 与 diagnostics 输出按 host 区分。

### R5 远端生命周期语义

- R5.1 `missing` 扫描按 host_id 限定，只扫本轮实际联系成功的主机。
- R5.2 `lossy_rebuild_risks` 排除不可达主机的 `missing` 行。
- R5.3 `source-status` 按主机输出只读三态：`idle`、`unreachable`、`never_contacted`。`live` 只出现在 sync 的 `SyncEvent` / `--json-events`，不进入 `source-status`。
- R5.4 `llmusage sync` 自动包含已注册远端；单台不可达时跳过、告警、退出码保持成功。
- R5.5 `remote sync [--host <label>]` 只同步远端、不跑本地 driver。该命令由 C2 实现；C4 把它接到 `llmusage sync` 的远端阶段，不重写命令。

### R6 文档与 spec

- R6.1 新增 ADR 记录远端主机导入决策，说明与 ADR 0011 的关系（passive parsing 仍是唯一解析机制，变化的是解析发生地）。
- R6.2 更新 `README.md`、`README.zh-CN.md`、`docs/index.md`、`docs/safety/index.md`、`docs/reference/cli.md` 及 `docs/zh/` 对应页。
- R6.3 更新 `.trellis/spec/llmusage/backend/` 下五份契约：token-accounting-contracts、source-sync-contracts、write-fencing-contracts、report-cli-contracts、dashboard-performance-contracts。

## Acceptance Criteria

- [ ] AC1 迁移前后：`usage_event` 行数不变；每 source 的 `total_tokens` 与 `cost_with_cache_usd` 合计不变；`usage_turn` 与 `usage_tool_call` 能关联到 `usage_event` 的行数不变。
- [ ] AC2 迁移后对同一份未变更的本地产物再次 `sync`，`events_inserted` 为 0。
- [ ] AC3 `reset_for_source(codex)` 只删除 codex 的 `usage_event_raw` 行，其他 source 的 raw 行全部保留。
- [ ] AC4 `remote add` 指向未安装 llmusage 的目标时返回明确错误，且 `host` 表不新增行。
- [ ] AC5 `remote add` 指向 shard 协议版本与本地不等的远端时拒绝注册，错误信息含两端协议版本；schema 版本可出现在诊断字段，单独的 schema 版本差异不得成为拒绝条件。
- [ ] AC6 远端与本地存在相同绝对路径的会话文件时，`source_file` 与 `source_cursor` 各自独立成行，cursor 不互相覆盖。
- [ ] AC7 远端不可达时 `llmusage sync` 完成本地同步、退出码为成功、输出含该主机告警，且该主机的 `source_file` 行不被扫为 `missing`。
- [ ] AC8 `--host` 过滤只返回该主机事件；不带 `--host` 时总量等于各主机之和。
- [ ] AC9 dashboard 主机分组的合计等于同条件下该主机 CLI 报表合计。
- [ ] AC10 shard NDJSON 不含 `raw_records`，且不含会话文本字段（断言序列化结果中无 prompt 正文）。
- [ ] AC11 远端 stdout 在 header 之前或记录之间混入非 JSON 行（登录横幅）时导入仍成功；被跳过的行数来自本地解析器并出现在告警中。
- [ ] AC12 `just ci` 通过。
- [ ] AC13 `sync --emit-shards` 结束后，用户库（`AppPaths.db_path`）的 schema_version、`usage_event` 行数、`source_file` 行数、`source_cursor` 行数均与运行前相同；该进程未在用户库上取得 worker lock。

## Out of Scope

- 并发同步多台远端（本期串行）。
- 远端二进制的自动安装或推送。
- host 与 source 的两级组合视图。
- 远端 raw archive 跨机器传输。
- 非 SSH 传输（HTTP、对象存储等）。
- 远端 dashboard 或远端写操作。

## Task Map

本任务为父任务，拆分为四个独立可验证的子任务：

| 子任务 | 覆盖需求 | 前置 |
|---|---|---|
| C1 host 维度 schema 与 event_key 前缀 | R2 | 无 |
| C2 SSH 传输与远端 shard 导入 | R1、R3、R5.5 | C1 |
| C3 读取层 host 维度 | R4 | C1 |
| C4 远端生命周期语义与文档 | R5.1–R5.4、R6 | C2 |

父任务负责跨子任务验收：AC8、AC9、AC12、AC13，以及最终集成审查。
