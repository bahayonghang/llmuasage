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

所有跨边界的字符串形式必须经过 `SourceKind::as_str()`，禁止散写 `"codex"` / `"claude"` / `"opencode"` 字面量。

新增源 = 在 `SourceKind` 加一个 variant，并在 [`Registry`](#10-registry) 注册一个 `SourceParser` + 一个 `Integration`。除此之外不应再有其他位置改动。

## 2. SourceParser

trait `SourceParser`（`src/parsers/source_parser.rs`）。每个源用一个 ZST 实现 trait（`CodexParser` / `ClaudeParser` / `OpencodeParser`），描述"如何把本地文件 / DB 解析成 [`SyncShard`](#7-syncshard) 并经 [`SyncRunWriter`](#8-syncrunwriter) 落库"。

签名：
```rust
fn source(&self) -> SourceKind;
fn parse<'a>(
    &'a self,
    store: &'a Store,
    writer: &'a mut SyncRunWriter,
    parallelism: usize,
) -> Pin<Box<dyn Future<Output = Result<SourceSyncStats>> + Send + 'a>>;
```

异步返回用 `Pin<Box<dyn Future>>` 显式包装，不引入 `async-trait`。详情见 [`docs/adr/0001-source-registry-and-parser-trait.md`](docs/adr/0001-source-registry-and-parser-trait.md)。

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

`pub struct SyncShard { source, reset_path_hashes, events, cursors }`（`src/store/mod.rs`）。

由 [`SourceParser`](#2-sourceparser) 产出、由 [`SyncRunWriter::commit_shard`](#8-syncrunwriter) 消费的原子写入单元。

字段语义：
- `reset_path_hashes`：在写入 events 前必须先清除其旧 events 的 path_hash 列表（用于"整文件重放"幂等）。
- `events`：本 shard 待写入的标准化 [`UsageEvent`](src/models.rs)。Writer 按 `EVENT_WRITE_BATCH_SIZE` 内部切 chunk。
- `cursors`：写完 events 后要落库的 [`FileCursor`](#5-cursor)。流式源（OpenCode）传空，自己用 `save_opencode_cursor` 单独收尾。

`SyncShard::new(source)` 是唯一构造器，没有 `Default` impl —— `source` 必须显式声明，避免静默走错 SQL key。详情见 [`docs/adr/0002-sync-shard-as-commit-protocol.md`](docs/adr/0002-sync-shard-as-commit-protocol.md)。

## 8. SyncRunWriter

`pub struct SyncRunWriter { conn: Connection }`（`src/store/sync_writer.rs`）。

单连接 sync 写入端。对外接口表面只有三个：
- `Store::begin_sync_run() -> Result<SyncRunWriter>`
- `SyncRunWriter::commit_shard(shard: SyncShard) -> Result<ShardCommitStats>`
- `SyncRunWriter::finish_sync_run()`

`commit_shard` 内部按 reset → chunked write_event → write_cursor 顺序串成原子动作；`reset_file_events_batch` / `write_event_batch` / `write_cursor_batch` 与 `EVENT_WRITE_BATCH_SIZE` 都是模块私有，parser 不感知。

`ShardCommitStats { events_inserted, write_ms }` 是回传的最小观测。

## 9. Store

`pub struct Store { pub paths: AppPaths }`（`src/store/mod.rs`）。

façade，本身只负责"持有路径 + 申请连接 + 申请 worker 锁 + bootstrap schema"。façade 直接方法只有：
- `Store::new(paths)`
- `Store::open_connection() -> Connection`
- `Store::acquire_worker_lock() -> Option<WorkerLock>`
- `Store::bootstrap()` / `Store::reset_usage_data()`
- `Store::begin_sync_run() -> SyncRunWriter`

所有领域数据访问通过 5 个借用 view 暴露：

| view-getter | 类型 | 表面 |
|------|------|------|
| `store.cursors()` | `CursorStore<'_>` | `source_cursor` 表 |
| `store.integration_state()` | `IntegrationStateStore<'_>` | `integration_install` 表 |
| `store.run_log()` | `RunLog<'_>` | `run_log` 表 |
| `store.sync_status()` | `SyncStatusStore<'_>` | `source_sync_status` 表 |
| `store.triggers()` | `TriggerStore<'_>` | `trigger_state` 表 |

view 全部是 `pub struct XxxStore<'a> { store: &'a Store }` 借用形态。详情见 [`docs/adr/0003-store-facade-vs-substores.md`](docs/adr/0003-store-facade-vs-substores.md)。

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
| [0003](docs/adr/0003-store-facade-vs-substores.md) | Store façade 与 5 个 view | Store, RunLog, CursorStore, IntegrationStateStore |

ADR 与本文档冲突时以 ADR 为准；同时本文档与 ADR 都需要更新。

## 编辑约定

- 新术语以"名词 + 一段定义 + 源文件锚点"的格式追加到本文件。
- 已存在术语含义变化时，更新本文件并在对应 ADR 末尾追加 `Updated: YYYY-MM-DD` 记录。
- 不要把本文件当成完整 API 文档；只放领域概念，不放方法签名细节。详细签名看源码或 ADR。
