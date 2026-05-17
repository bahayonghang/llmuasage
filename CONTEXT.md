# CONTEXT — llmusage 领域术语

本文件是 llmusage 仓库的单点术语表（single-context glossary）。所有 issue、PR、ADR、commit message 与代码注释引用领域概念时，必须使用本文件定义的术语，不要漂移到同义词。

适用范围：本仓库 Rust 代码、`docs/` VitePress 站点、`AGENTS.md` 与 `docs/agents/`。

如果一个新概念需要进入本表，先在 PR 描述里说明它在哪个层次（数据 / 协议 / 接口 / 命令）；如果与已存在术语冲突，优先扩展旧术语而不是新造一个。

---

## 1. Source

枚举 `SourceKind`（`src/models.rs`），llmusage 当前支持的本地真源：

- `Codex`：OpenAI Codex 本地 rollout / session JSONL。
- `Claude`：Claude Code 本地 project JSONL。
- `Opencode`：OpenCode 本地 SQLite。
- `Gemini`：Gemini CLI 本地 chat session JSON。

所有跨边界的字符串形式必须经过 `SourceKind::as_str()`，禁止散写 `"codex"` / `"claude"` / `"opencode"` / `"gemini"` 字面量。

新增源 = 在 `SourceKind` 加一个 variant，并在 [`Registry`](#10-registry) 注册一个 `SourceParser` + 一个 `Integration`。除此之外不应再有其他位置改动。

## 2. SourceParser

trait `SourceParser`（`src/parsers/source_parser.rs`）。每个源用一个 ZST 实现 trait（`CodexParser` / `ClaudeParser` / `OpencodeParser` / `GeminiParser`），描述"如何把本地文件 / DB 解析成 [`SyncShard`](#7-syncshard) 并经 [`SyncRunWriter`](#8-syncrunwriter) 落库"。

签名：
```rust
fn source(&self) -> SourceKind;
fn parse<'a>(
    &'a self,
    store: &'a Store,
    writer: &'a mut SyncRunWriter,
    parallelism: usize,
    cancel: &'a CancellationToken,
    progress: Option<ProgressSink<'a>>,
) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>>;
```

异步返回用 `Pin<Box<dyn Future>>` 显式包装，不引入 `async-trait`。`cancel` 来自 M2 JobRegistry / `sync --json-events` 通路；driver 在 parser 边界检查，parser 在文件或分页边界检查。`progress` 是 parser 内部发现文件数与 shard/page 提交后的 `SyncEvent` 回调，缺省为 `None`，用于 CLI stderr / NDJSON / JobRegistry 进度可见性。详情见 [`docs/adr/0001-source-registry-and-parser-trait.md`](docs/adr/0001-source-registry-and-parser-trait.md)。

驱动端是 `parsers::driver::drive(...)`，按注册顺序串行调用每个 parser，并统一注入 `lock_wait_ms`。

## 3. Integration

trait `Integration`（`src/integrations/integration.rs`）。每个源用一个 ZST 实现 trait（`CodexIntegration` / `ClaudeIntegration` / `OpencodeIntegration`），描述"如何在外部工具的本地配置里安装 / 卸载 / 探测 llmusage hook 包装器"。

签名：
```rust
fn source(&self) -> SourceKind;
fn probe(&self, app: &AppContext) -> Result<IntegrationProbe>;
fn install(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction>;
fn uninstall(&self, app: &AppContext, store: &Store) -> Result<IntegrationAction>;
```

签名为同步 `&self`：probe / install / uninstall 三动作都是 fs / json / toml 的同步操作，不需要 await。

`probe_all` / `install_all` / `uninstall_all`（`src/integrations/mod.rs`）退化为对 [`Registry`](#10-registry) 的遍历。

## 4. HookTarget

`pub struct HookTarget { kind: HookKind, path: PathBuf }`（`src/integrations/hook_target.rs`）。

唯一聚合 `cfg!(windows)` 的入口。`Integration::install` 等所有需要拼装 shell 命令 / notify argv 的位置都必须经 `HookTarget::current(app)` 拿到 target，然后调 `shell_command(source, trigger)` 或 `notify_args(source, trigger)`。

`HookKind`：
- `WindowsCmd` → `cmd /c <hook>.cmd ...`
- `UnixSh` → `/usr/bin/env sh <hook>.sh ...`

调用方禁止直接写 `cfg!(windows)`、禁止直接拼 `cmd /c` 或 `/usr/bin/env sh` 字符串。

## 5. Cursor

每源的增量游标：

- `FileCursor`（`src/store/mod.rs`）：file-backed JSONL 源用（Codex / Claude）。键为 cursor_key，含 fingerprint / size / mtime_ns / tail_signature / offset / last_total / last_model。
- `OpencodeCursor`（`src/store/mod.rs`）：OpenCode SQLite 源用。键为 inode + last_time_created + last_processed_ids。

游标的读写表面在 [`CursorStore<'a>`](#9-store)。Cursor 决定"下一次 sync 从哪里继续"，是 sync 增量性的全部依据。

## 6. Bucket

30 分钟 UTC 窗口。表 `usage_bucket_30m` 是所有 dashboard / report 查询的聚合源。每条 [`UsageEvent`](src/models.rs) 落库时按 `(source, model, hour_start, project_hash)` 升级到对应 bucket（结构体 `BucketKey` / `BucketRollup` 在 `src/store/mod.rs` 内部）。

`hour_start` 字段命名保留历史命名，含义是"30 分钟窗口的起点"，不是"小时起点"。

## 7. SyncShard

`pub struct SyncShard { source, reset_path_hashes, events, cursors, seen_file_paths, raw_records, turns, tool_calls }`（`src/store/mod.rs`）。

由 [`SourceParser`](#2-sourceparser) 产出、由 [`SyncRunWriter::commit_shard`](#8-syncrunwriter) 消费的原子写入单元。

字段语义：
- `reset_path_hashes`：在写入 events 前必须先清除其旧 events 的 path_hash 列表（用于"整文件重放"幂等）。
- `events`：本 shard 待写入的标准化 [`UsageEvent`](src/models.rs)。Writer 按 `EVENT_WRITE_BATCH_SIZE` 内部切 chunk。
- `cursors`：写完 events 后要落库的 [`FileCursor`](#5-cursor)。流式源（OpenCode）传空，自己用 `save_opencode_cursor` 单独收尾。
- `seen_file_paths`：parser 本轮看见的源文件路径，writer 同事务标记为 [`SourceFile`](#17-sourcefile) live。
- `raw_records`：可选 raw archive 记录，仅在 raw archive 开启时写入 `usage_event_raw`。
- `turns`：标准化 [`UsageTurn`](#25-usageturn) 行为事实，用于 Activity / Optimize / Compare 查询；parser 可在不支持时留空。
- `tool_calls`：标准化 [`UsageToolCall`](#26-usagetoolcall) 工具/动作事实，用于 Tools / Optimize / Compare 查询；parser 可在不支持时留空。

`SyncShard::new(source)` 是唯一构造器，没有 `Default` impl —— `source` 必须显式声明，避免静默走错 SQL key。详情见 [`docs/adr/0002-sync-shard-as-commit-protocol.md`](docs/adr/0002-sync-shard-as-commit-protocol.md)。

## 8. SyncRunWriter

`pub struct SyncRunWriter { conn: Connection }`（`src/store/sync_writer.rs`）。

单连接 sync 写入端。对外接口表面只有三个：
- `Store::begin_sync_run() -> Result<SyncRunWriter>`
- `SyncRunWriter::commit_shard(shard: SyncShard) -> Result<ShardCommitStats>`
- `SyncRunWriter::finish_sync_run()`

`commit_shard` 内部按 reset → behavior reset → chunked write_event → source_file live → cursor → raw archive → behavior facts 顺序串成原子动作；`reset_file_events_batch` / `write_event_batch` / `write_cursor_batch` 与 `EVENT_WRITE_BATCH_SIZE` 都是模块私有，parser 不感知。

`ShardCommitStats { events_inserted, write_ms, files_seen, turns_inserted, tool_calls_inserted }` 是回传的最小观测。

## 9. Store

`pub struct Store { pub paths: AppPaths }`（`src/store/mod.rs`）。

façade，本身只负责"持有路径 + 申请连接 + 申请 worker 锁 + bootstrap schema"。façade 直接方法只有：
- `Store::new(paths)`
- `Store::open_connection() -> Connection`
- `Store::acquire_worker_lock() -> Option<WorkerLock>`（hook-run 兼容非阻塞入口）
- `Store::acquire_worker_lock_with(timeout, HolderKind) -> WorkerLock`（CLI/library 阻塞入口）
- `Store::current_worker_lock() -> Option<WorkerLockMeta>`
- `Store::bootstrap()` / `Store::reset_usage_data()`
- `Store::begin_sync_run() -> SyncRunWriter`

所有领域数据访问通过借用 view 暴露。0.4.x 有 5 个 view；0.5.0 追加 `SourceFileStore` 作为第 6 个 view：

| view-getter | 类型 | 表面 |
|------|------|------|
| `store.cursors()` | `CursorStore<'_>` | `source_cursor` 表 |
| `store.integration_state()` | `IntegrationStateStore<'_>` | `integration_install` 表 |
| `store.run_log()` | `RunLog<'_>` | `run_log` 表 |
| `store.sync_status()` | `SyncStatusStore<'_>` | `source_sync_status` 表 |
| `store.triggers()` | `TriggerStore<'_>` | `trigger_state` 表 |
| `store.source_files()` | `SourceFileStore<'_>` | `source_file` 表（0.5.0 / ADR 0006） |

view 全部是 `pub struct XxxStore<'a> { store: &'a Store }` 借用形态。跨表事务、migration、worker lock、pricing recompute、source-file forget 等仍是 façade 直接方法。详情见 [`docs/adr/0003-store-facade-vs-substores.md`](docs/adr/0003-store-facade-vs-substores.md)。

## 10. Registry

`src/sources.rs::registered_parsers()` / `registered_integrations()`。

工厂返回 `Vec<Box<dyn SourceParser>>` / `Vec<Box<dyn Integration>>`，是"系统支持哪些源"的唯一真源。`commands::sync::run_once` 与 `integrations::{probe_all, install_all, uninstall_all}` 都对 registry 做遍历。

新增源时 registry 是必经入口；忘记在 registry 注册 = 该源对系统不可见。

## 11. RunLog

`pub struct RunLog<'a> { store: &'a Store }`（`src/store/run_log.rs`）+ `RunRecord`（`src/store/mod.rs`）。

每次 CLI 命令执行（`sync` / `init` / `uninstall` / `serve` / `hook-run` 等）的生命周期记录。字段：`id` / `command` / `status`（`running` / `success` / `failed` / `skipped` / `aborted`） / `summary` / `error` / `started_at` / `finished_at`。

是 `status` / `doctor` / `diagnostics` / web `/api/health` 共用的"最近运行"数据源；`recover_running_runs` 用于把上次 crash 留下的 `running` 记录翻成 `aborted`。

---

## 与 ADR 的关系

| ADR | 主题 | 主要术语 |
|------|------|---------|
| [0001](docs/adr/0001-source-registry-and-parser-trait.md) | SourceParser trait + Registry | Source, SourceParser, Integration, Registry |
| [0002](docs/adr/0002-sync-shard-as-commit-protocol.md) | SyncShard 作为 commit 协议 | SyncShard, SyncRunWriter, Cursor, Bucket |
| [0003](docs/adr/0003-store-facade-vs-substores.md) | Store façade 与 view | Store, RunLog, CursorStore, IntegrationStateStore, SourceFileStore |
| [0004](docs/adr/0004-schema-version-migration-runner.md) | schema_version + migration runner | Migration, SchemaVersion, Store |
| [0005](docs/adr/0005-job-registry-in-memory.md) | JobRegistry 内存态任务编排 | Job, JobRegistry, JobSnapshot |
| [0006](docs/adr/0006-source-file-state-machine.md) | source_file 三态状态机 | SourceFile, FileState, Cursor |
| [0007](docs/adr/0007-llmusage-error-surface.md) | LlmusageError 公共错误表面 | LlmusageError |
| v11 behavior facts | Activity / Tools / Optimize / Compare normalized 行为事实 | UsageTurn, UsageToolCall, SyncShard, QueryFilter |
| v12 source_sync_status repair | 修复 schema_version 已推进但 `source_sync_status.stored_events` 缺失的历史库漂移 | Migration, SchemaVersion, Store |

ADR 与本文档冲突时以 ADR 为准；同时本文档与 ADR 都需要更新。

## 编辑约定

- 新术语以"名词 + 一段定义 + 源文件锚点"的格式追加到本文件。
- 已存在术语含义变化时，更新本文件并在对应 ADR 末尾追加 `Updated: YYYY-MM-DD` 记录。
- 不要把本文件当成完整 API 文档；只放领域概念，不放方法签名细节。详细签名看源码或 ADR。


## 12. Migration

`Migration` 是一次有版本号的 SQLite schema/data 升级步骤（0.5.0 起，见 `src/store/migrations.rs` 与 ADR 0004）。每步由 `(version, name, fn(&Transaction) -> Result<()>)` 描述，按版本号升序执行，并在独立事务内把 [`SchemaVersion`](#13-schemaversion) 写到目标版本。

约束：
- v1 是 `baseline`，负责把 0.4.x 的 `Store::bootstrap()` 建表/ensure-column 逻辑搬进 migration runner。
- v2+ 必须是真实 schema/data 变更，不允许为了“凑 latest”写空占位。
- migration 失败必须回滚当前事务，备份保留，不实现 down migration。

## 13. SchemaVersion

`SchemaVersion` 是 `meta('schema_version', N)` 里的整数版本号。读不到 `meta` 或读不到 `schema_version` 时按 v0 处理，即 0.4.x 老库。

阶段边界：
- M0- 只允许推进到 v1 baseline。
- v2-v10 随 M1/M2/M3 的真实 migration 逐步追加。
- 0.5.0 final 才要求 schema_version == 10。
- 0.6.x 行为事实追加 v11；v12 只做 `source_sync_status` 历史列兼容修复，不重建表、不删除数据。

## 14. Job

`Job` 是一次 in-process usage import / sync 任务。它不是持久化实体，进程退出后 job 状态消失；真实可恢复状态仍由 SQLite 中的 usage/cursor/run-log 表承担。

ccr-ui 通过 `start_usage_import_job` 创建 job，通过 `get_usage_import_job` 轮询 [`JobSnapshot`](#16-jobsnapshot)，通过 `cancel_usage_import_job` 触发取消。

## 15. JobRegistry

`JobRegistry` 是 0.5.0 引入的内存态 job 登记表（ADR 0005）。推荐结构是 `Arc<DashMap<JobId, Arc<Mutex<JobState>>>>`。它把 `SyncEvent` push 流转换为可轮询的 `JobSnapshot`。

约束：
- M0- 只落类型/签名雏形，不真正启动 sync task。
- M2 才实现 start/get/cancel 的完整生命周期。
- `Mutex<JobState>` 不能跨 await；snapshot 必须 clone 后立即释放锁。

## 16. JobSnapshot

`JobSnapshot` 是对外暴露的 job 当前状态值对象，字段使用 snake_case serde。它只描述当前进程内 job 的最近观测值，不承诺跨进程/重启恢复。

典型状态：running / completed / failed / cancelled。completed/failed/cancelled 后可被 `list_recent` 容量策略淘汰，running job 不淘汰。

## 17. SourceFile

`SourceFile` 是 `source_file` 表中的一条源文件状态记录（ADR 0006），主键为 `(source, file_path)`。它不是 cursor；它描述“这个源文件在诊断意义上是否仍然存在/有效”。

写入时机：
- parser/driver 看见文件并 commit shard 后，同事务 upsert 为 live。
- 每个 source 扫描结束后，上一轮 live 但本轮没看见的记录转为 missing。
- 用户通过 forget 入口标记 deleted_by_user 时，同时删除对应 source_cursor。

## 18. FileState

`FileState` 是 `SourceFile.file_state` 的三态枚举：
- `live`：最近一次扫描看见并接受该文件。
- `missing`：曾经 live，但最近扫描没看见。
- `deleted_by_user`：用户显式选择忽略/遗忘该文件。

状态转换必须遵循 ADR 0006：扫描看见任何旧状态都回 live；扫描没看见只把 live 转 missing；forget 覆盖为 deleted_by_user。

## 19. LlmusageError

`LlmusageError` 是 0.5.0 的公共错误枚举（ADR 0007），替代公开 API 里的 `anyhow::Result<T>`。它服务于 ccr-ui/Tauri 适配层按错误类型映射 UI，而不是替代 CLI 内部的所有 anyhow context。

约束：
- enum 必须 `#[non_exhaustive]`。
- 公开 API 返回 `Result<T, LlmusageError>`。
- CLI/parsers 内部可继续用 anyhow，但跨 crate 边界前要转换。

## 20. QueryFilter

`QueryFilter` 是 dashboard/report/home/heatmap/logs 查询共享的过滤条件对象。字段包括 source、model、since、until、project_hash、timezone。默认 timezone 是 local。

它的职责是把“ccr-ui 传入的筛选条件”稳定映射到 query 层，而不是让每个 dashboard 方法各自解析参数。

## 21. UsageArchiveDiagnostics

`UsageArchiveDiagnostics` 是诊断面板使用的归档/源文件状态聚合结果。M0 阶段可返回占位：`archive_root` + 空 `by_source` + run_log 失败摘要；M2 的 [`SourceFile`](#17-sourcefile) / [`FileState`](#18-filestate) 落地后，`by_source` 才填 live/missing/deleted 三态计数。

rc 语义：rc.1/rc.2 的 `by_source` 允许为空；rc.3 起该字段必须稳定。

## 22. PricingCatalog

`PricingCatalog` 是本地价格目录（`src/query/pricing_catalog.rs`）。默认静态目录来自 `pricing/static-v1.json`；`llmusage doctor --refresh-pricing <file>` 只能读取用户提供的本地 JSON snapshot，不联网抓取。

约束：
- `compute_cost` / `recompute_costs` 默认使用 static v1。
- `compute_cost_with` / `recompute_costs_with` 用于本地 snapshot 注入。
- migration v10 与 `doctor --refresh-pricing` 都必须写 `meta('pricing_catalog_version')`，让下游 UI 能区分 static 与 snapshot 成本。

## 23. WorkerLock

`WorkerLock` 是全局本地 worker 锁（SQLite 表 `worker_lock`）。0.5.0 起锁元信息包含 `holder_pid`、`holder_kind` 和 `acquired_at`。

约束：
- CLI sync 与 library `JobRegistry` 使用 `Store::acquire_worker_lock_with(timeout, HolderKind::{Cli,Library})`，等待同一把锁。
- `hook-run` 保留旧的非阻塞 `Store::acquire_worker_lock()`；锁被占用时跳过，避免高频 hook 堆积。
- `LlmusageError::LockBusy.holder` 是字符串，不暴露 `WorkerLockMeta` 公共结构。

## 24. TestingFixture

`testing::Fixture` 是 0.5.0 暴露给下游 adapter 测试使用的 feature-gated 本地夹具（`features = ["testing"]`）。它创建隔离 runtime root、bootstrap SQLite，并提供 `seed_event` / `seed_dashboard`。

约束：
- `testing` feature 依赖 optional `tempfile`；普通库构建不暴露该模块。
- `seed_event` 必须同时写 `usage_event` 与 `usage_bucket_30m`，因为 dashboard 主读路径聚合 bucket。

## 25. UsageTurn

`UsageTurn` 是 `usage_turn` 表中的一条 turn-level 行为事实（0.6.x / migration v11）。它由 parser 放入 [`SyncShard`](#7-syncshard) 的 `turns`，并由 [`SyncRunWriter`](#8-syncrunwriter) 落库。

它不是成本主事实表；成本/用量主路径仍是 [`UsageEvent`](src/models.rs) 与 [`Bucket`](#6-bucket)。`UsageTurn` 只为 Activity / Optimize / Compare 提供 normalized 行为维度：source、session_id、source_path_hash、project_hash、primary_model、started_at、category、has_edits、retries、one_shot、call_count 与 token 汇总。

隐私约束：不得保存完整 user prompt、完整 assistant text 或完整文件内容。无法可靠提取行为的 source 可以只写 conservative one-event turn，或留空并让查询层返回 explicit no_data / degraded support。

## 26. UsageToolCall

`UsageToolCall` 是 `usage_tool_call` 表中的一条工具/动作行为事实（0.6.x / migration v11）。它由 parser 放入 [`SyncShard`](#7-syncshard) 的 `tool_calls`，并由 [`SyncRunWriter`](#8-syncrunwriter) 落库。

它服务于 Tools、Optimize、Compare 查询，字段包括 tool_call_key、turn_key、event_key、source、session_id、source_path_hash、project_hash、model、occurred_at、tool_name、tool_kind、mcp_server、mcp_tool、input_fingerprint 与 safe_preview。

隐私约束：`safe_preview` 只能是 bounded、显示安全的路径/命令/查询摘要；`input_fingerprint` 用于重复读取等检测，不能反向还原完整输入。Optimize 基于这些事实生成只读建议，不能自动删除、移动、归档或重写用户文件。
