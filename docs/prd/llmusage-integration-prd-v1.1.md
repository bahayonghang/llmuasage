# llmusage × ccr-ui 集成 PRD v1.1

> 文档身份：v1.1 = v1 + audit 12 项缺口 + 30 项 grilling 决议的合订本。
> v1 / audit 仍保留作变更证据。
> 适用版本：llmusage 0.5.0（首个 SemVer-stable 切线）。
> 目标读者：llmusage 维护者、ccr-ui 适配层维护者。

---

## 0. TL;DR

ccr-ui **完全弃用** `ccr-db::usage_repo` + `ccr-store::cost_tracker` 这条用量统计管线。改为：

1. 直接读 llmusage 维护的本地 SQLite（`AppPaths::with_root` 指定路径）做仪表盘查询。
2. 通过 `JobRegistry` 触发并轮询 `llmusage::sync` 任务。
3. ccr-ui Vue 契约（`ccr-ui/src/types/usage.ts`）字段不变；适配层只做参数翻译 + 透传 + 补 session 字段。

llmusage 0.5.0 一次性交付：4 张新表 / 6 个新列 / 1 套 thiserror enum / 1 个 JobRegistry / 8 个 HTTP endpoint / 1 个 Gemini 源 / 1 套 versioned migration。snake_case 全口径 casing 切线。

---

## 1. 目标 / 非目标

### 1.1 目标
- llmusage 成为 ccr-ui 用量统计的**唯一数据源**。
- ccr-ui 通过 lib（首选）或子进程（备选）触发 `sync` 与读 dashboard。
- ccr-ui Vue 字段不变。

### 1.2 非目标
- 不复刻 ccr-db schema；ccr-ui 接受字段重命名。
- 不替代 ccr-ui 的 `SessionIndexer`。
- 不做 ccr-db 历史数据 importer（D7）。
- 不引 OpenTelemetry（D23）。
- 不联网刷价目（D21）。

---

## 2. 目标架构

```text
┌───────────────┐        ┌───────────────────────────┐
│  ccr-ui       │  read  │ llmusage SQLite           │ ◄── hooks (Codex notify
│  Vue + Tauri  │ ─────► │ <root_dir>/llmusage.db    │     Claude Stop / SessionEnd
│               │        │ root_dir 由 with_root 指定 │     OpenCode plugin
│               │        └───────────────────────────┘     Gemini SessionEnd)
│               │  trigger sync
│               │ ─────► llmusage::sync::run_with_progress (lib)
│               │ ─────► llmusage sync --json-events (subprocess)
│               │
│               │  poll job
│               │ ─────► JobRegistry::snapshot(job_id)
│               │ ◄───── JobSnapshot { state, stats, events_recent }
└───────────────┘
```

ccr-ui Tauri 层：参数翻译 + 透传 + 补 session 字段。

---

## 3. 决议表（D1–D30）

```text
┌─────┬──────────────────────────────┬───────────────────────────────────────┐
│ #   │ 主题                         │ 拍板                                  │
├─────┼──────────────────────────────┼───────────────────────────────────────┤
│ D1  │ migration 框架               │ 自家 versioned + meta.schema_version  │
│ D2  │ breaking 批次                │ 0.5.0 一次性 + 0.5.0-rc 预发布        │
│ D3  │ 路径优先级                   │ with_root > --home > env > default    │
│ D4  │ JobRegistry                  │ 内存 only                             │
│ D5  │ cancel 颗粒度                │ file 边界                             │
│ D6  │ cost 重算                    │ refresh-pricing 末尾全量重算          │
│ D7  │ ccr-db 历史                  │ 仅 sync 重建，不写 importer           │
│ D8  │ cache_efficiency             │ 跨源统一公式                          │
│ D9  │ output 与 reasoning          │ API 合并 / CLI 可拆                   │
│ D10 │ pricing_source 格式          │ 单字段 "<catalog>-<version>"          │
│ D11 │ OpenCode raw                 │ row→JSON 序列化                       │
│ D12 │ home_overview 责任           │ llmusage usage 子集 + ccr-ui 补 sess  │
│ D13 │ WorkerLock 元信息            │ pid + kind + acquired_at              │
│ D14 │ Gemini hook wrapper          │ 复用 hook_target 路径生成             │
│ D15 │ file_state 入口              │ 完整三态 + CLI/API/HTTP 三入口        │
│ D16 │ testing Fixture              │ feature="testing" 同 crate            │
│ D17 │ LlmusageError                │ 8 variant 粗粒度                      │
│ D18 │ JSON casing                  │ 一刀切 snake_case                     │
│ D19 │ timezone 默认                │ Local                                 │
│ D20 │ reset_for_source             │ 事务删 6 表，project_dim 不动         │
│ D21 │ pricing 上游                 │ LiteLLM model_prices_*.json           │
│ D22 │ 阶段验收                     │ Tauri 命令集通车                      │
│ D23 │ OTel                         │ 0.5.0 不进                            │
│ D24 │ HTTP API                     │ 与 Tauri 同步加 8 endpoints           │
│ D25 │ project_path                 │ 落原文                                │
│ D26 │ logs cursor                  │ base64url(JSON)                       │
│ D27 │ RecentReady                  │ 每源独立发                            │
│ D28 │ archive_root                 │ root_dir 路径                         │
│ D29 │ 0.4→0.5 升级                 │ 自动迁 + 迁前备份                     │
│ D30 │ ADR / CONTEXT 同步           │ 4 新 ADR + CONTEXT 加 8 术语          │
└─────┴──────────────────────────────┴───────────────────────────────────────┘
```

---

## 4. 字段级 GAP（v1.1 终版）

```text
┌─────────────────────────────────┬──────────────┬──────────────────────────┐
│ ccr-ui 契约                     │ 0.5.0 来源   │ 备注                     │
├─────────────────────────────────┼──────────────┼──────────────────────────┤
│ UsageSummary.total_requests     │ overview     │ event_count（F1.4）      │
│ UsageSummary.total_input_tokens │ overview     │ 直接                     │
│ UsageSummary.total_output_      │ overview     │ output+reasoning（D9）   │
│   tokens                        │              │                          │
│ UsageSummary.total_cache_read_  │ overview     │ cached→cache_read（D8）  │
│   tokens                        │              │                          │
│ UsageSummary.total_cost_usd     │ overview     │ cost_with_cache_usd      │
│ UsageSummary.cache_efficiency   │ overview     │ Σread/(Σin+Σread)（D8）  │
│ DailyTrend.*                    │ trends_daily │ 完整分项（F4.2）         │
│ ModelStat.cost_with_cache /     │ model_break- │ CostBreakdown（D6/D10）  │
│   without_cache / cache_savings │ down         │                          │
│ ModelStat.pricing_status /      │ model_break- │ PricingCatalog（D10/21） │
│   pricing_source / pricing_rate │ down         │                          │
│ UsageRecordV2.record_json       │ logs         │ raw_archive opt-in (D11) │
│ UsageRecordV2.platform /        │ logs         │ source_path_hash 关联    │
│   project_path / source_id      │              │ (D25)                    │
│ PaginatedLogs cursor + offset   │ logs         │ base64url JSON (D26)     │
│ HeatmapResponse                 │ heatmap      │ daily count + tokens     │
│ UsageArchiveDiagnostics.*       │ diagnostics  │ source_file 状态机       │
│                                 │              │ (D15/D28)                │
│ Codex 重导（按源）              │ sync         │ --rebuild --source (D20) │
│ 实时进度（job-progress）        │ JobRegistry  │ Started/Progress/Recent- │
│                                 │              │ Ready/Finished/Failed    │
│ Gemini 平台                     │ Gemini parser│ chats JSON + hook (D14)  │
│ HomeUsageOverviewResponse       │ home_overview│ usage 子集 (D12)         │
└─────────────────────────────────┴──────────────┴──────────────────────────┘
```

---

## 5. 功能需求

### F0 基建（M0- 阶段）

#### F0.1 schema_version + migration runner（D1, ADR 0004）

新增 `meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)` 表，固定行 `meta('schema_version', 'N')`。`bootstrap()` 流程：

```text
1. open_connection
2. ensure meta exists
3. read schema_version (NULL → v0)
4. if v0 detected → cp db_path 到 backups/llmusage.db.pre-0.5.0
5. for (v, name, fn) in MIGRATIONS where v > current:
       BEGIN; fn(tx)?; meta.set('schema_version', v); COMMIT
6. info!("schema at v{N}")
```

`MIGRATIONS` 是 `&[(u32, &str, fn(&Transaction) -> Result<()>)]`。baseline (v1) 必须 idempotent（与现有 0.4.x 表共存）。

migration 列表（v1.1 起的目标顺序）：

```text
v1  baseline                 // 与 0.4.x 表对齐
v2  add_cache_split          // cache_creation_tokens 列
v3  add_cost_breakdown       // cost_with_cache_usd / cost_without_cache_usd /
                             // pricing_status / pricing_source / pricing_rate
v4  add_event_count_proj     // project_path 列, source_path_hash 索引
v5  add_source_file          // source_file 表（live/missing/deleted_by_user）
v6  add_recent_history       // source_sync_status.recent_completed_at /
                             // history_completed_at
v7  add_raw_archive          // usage_event_raw 表 + meta('raw_archive_enabled')
v8  add_worker_lock_meta     // worker_lock.holder_pid / holder_kind / acquired_at
v9  add_gemini               // 无 schema 改动，仅 SourceKind 注册
v10 add_pricing_meta         // meta('pricing_catalog_version') 等
```

#### F0.2 JobRegistry（D4, ADR 0005）

```rust
pub struct JobRegistry {
    inner: Arc<DashMap<JobId, Arc<Mutex<JobState>>>>,
}

#[derive(Clone, Serialize)]
pub struct JobSnapshot {
    pub job_id: String,
    pub status: JobStatus,           // pending|running|recent_ready|completed|failed|cancelled
    pub started_at: String,
    pub updated_at: String,
    pub recent_ready_at: Option<String>,
    pub finished_at: Option<String>,
    pub current_file: Option<String>,
    pub stats: JobStats,             // files_total / files_scanned / records_imported / records_skipped
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub results: Vec<SourceSyncStats>,
}

impl JobRegistry {
    pub fn start(&self, store: &Store, opts: SyncOptions)
        -> Result<(JobId, mpsc::Receiver<SyncEvent>), LlmusageError>;
    pub fn snapshot(&self, id: &str) -> Option<JobSnapshot>;
    pub fn cancel(&self, id: &str) -> bool;
    pub fn list_recent(&self, limit: usize) -> Vec<JobSnapshot>;
}
```

进程内存活，进程退出即丢。ccr-ui Tauri 重启 = 重发 import。

#### F0.3 LlmusageError（D17, ADR 0007）

```rust
#[derive(thiserror::Error, Debug)]
pub enum LlmusageError {
    #[error("io: {0}")]            Io(#[from] std::io::Error),
    #[error("db: {0}")]            Db(#[from] rusqlite::Error),
    #[error("parse: {0}")]         Parse(String),
    #[error("worker lock busy: held by pid {pid} kind {kind} since {since}")]
                                    LockBusy { pid: i32, kind: String, since: String },
    #[error("not initialized — run `llmusage init`")]
                                    NotInitialized,
    #[error("config invalid: {0}")] ConfigInvalid(String),
    #[error("migration {version} failed: {source}")]
                                    MigrationFailed { version: u32, source: anyhow::Error },
    #[error("pricing missing for source={source} model={model}")]
                                    PricingMissing { source: String, model: String },
}
```

公开 API（query / sync / store façade / paths）改 `Result<T, LlmusageError>`。CLI 内部仍 `anyhow::Result` + context，仅边界转。

#### F0.4 路径解析（D3）

```rust
impl AppPaths {
    pub fn discover() -> Result<Self>;                 // env > default
    pub fn with_root(root: PathBuf) -> Result<Self>;   // 编程入口最高优先
    pub fn with_cli_home(home: Option<PathBuf>) -> Result<Self>;
                                                       // CLI flag --home
}

// 实际优先级：
// AppPaths::with_root(p)  >  --home p  >  $LLMUSAGE_HOME  >  ~/.llmusage
```

#### F0.5 Cargo 元信息

```toml
[lib]
name = "llmusage"
path = "src/lib.rs"

[[bin]]
name = "llmusage"
path = "src/main.rs"

[features]
default = []
testing = []   # 暴露 src/testing/Fixture（D16）
```

### F1 数据模型扩展

#### F1.1 Gemini 源（D14）

主路径解析 `~/.gemini/tmp/<projectHash>/chats/session-*.json`。字段：每条 assistant 消息读 `tokens.{input,output,cached}` 或 `usageMetadata` / `usage_metadata`。`thoughtsTokenCount` → `reasoning_output_tokens`。项目识别：`~/.gemini/projects.json` 反查 `projectHash` → cwd → `project::resolve_project_info`。

可选 hook（推荐安装）：`~/.gemini/settings.json::hooks.SessionEnd`，调用 `~/.llmusage/bin/llmusage-hook.cmd|sh`，与 Claude/Codex 一致复用 `hook_target.rs`（D14）。

注册：`SourceKind::Antigravity`（`as_str() == "antigravity"`），`registry::registered_parsers / registered_integrations` 各加一行。

#### F1.2 cache 拆分（D8）

```rust
pub struct UsageTokens {
    pub input_tokens: i64,
    pub cache_read_tokens: i64,        // ← 原 cached_input_tokens
    pub cache_creation_tokens: i64,    // ← 仅 Anthropic 非 0
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}
```

`usage_event` / `usage_bucket_30m` 加列 `cache_creation_tokens INTEGER NOT NULL DEFAULT 0`（migration v2）。`parsers/claude.rs` 同时读 `cache_creation_input_tokens` 和 `cache_read_input_tokens`。其他源 `cache_creation_tokens = 0`。

cache_efficiency 公式（D8）：

```text
cache_efficiency = Σ cache_read_tokens
                  ────────────────────────────────────────
                   Σ input_tokens + Σ cache_read_tokens
```

跨源统一，不区分。OverviewPayload.cache_efficiency 类型保持 `f64`（不 Option），在分母为 0 时返 0.0。

#### F1.3 双价 + 定价元数据（D6, D10, D21）

```rust
pub struct CostBreakdown {
    pub cost_with_cache_usd: f64,
    pub cost_without_cache_usd: f64,
    pub pricing_status: PricingStatus,           // priced | partial | unpriced
    pub pricing_source: Option<String>,          // "static-v1" | "litellm-snapshot-2026-04" | None
    pub pricing_rate: Option<String>,            // "input=$3/MTok, cache=$0.3/MTok, output=$15/MTok"
}
```

`usage_event` 加列：`cost_with_cache_usd / cost_without_cache_usd / pricing_status / pricing_source / pricing_rate`（migration v3）。bucket 聚合 sum；`Dashboard` 派生 `cache_savings = cost_without_cache - cost_with_cache`。

价目表生命周期：

```text
┌─────────────────────────┬──────────────────────────────────────────┐
│ 来源                    │ pricing_source 取值                      │
├─────────────────────────┼──────────────────────────────────────────┤
│ pricing/static-v1.json  │ "static-v1"                              │
│ （编译期 embed）        │                                          │
│ ~/.llmusage/pricing/    │ "litellm-snapshot-YYYY-MM"               │
│   litellm-snapshot-     │ （文件名嵌版本）                         │
│   YYYY-MM.json          │                                          │
│ 命中失败                │ None + pricing_status="unpriced"+cost=0  │
└─────────────────────────┴──────────────────────────────────────────┘
```

`llmusage doctor --refresh-pricing <path|url>` 做四件事：

```text
1. 校验 JSON 合法 + 字段格式
2. cp 到 ~/.llmusage/pricing/litellm-snapshot-YYYY-MM.json
3. 写 meta('pricing_catalog_version', 'YYYY-MM')
4. 跑 Store::recompute_costs()  ← D6 全量重算
```

llmusage 不直连远端。CI 可加 cron 拉 BerriAI/litellm 提 PR。

#### F1.4 总请求数（event count）

`OverviewPayload` 加 `total_events: i64` 与 `last_24h_events: i64`。`model_breakdown / source_breakdown / project_breakdown / cost_breakdown` 全部加 `event_count`。

#### F1.5 raw_archive opt-in（D11）

```rust
pub struct BootstrapOptions { pub enable_raw_archive: bool /* default false */ }

impl Store {
    pub fn bootstrap(&self) -> Result<(), LlmusageError>;
    pub fn bootstrap_with(&self, opts: BootstrapOptions) -> Result<(), LlmusageError>;
    pub fn raw_archive_enabled(&self) -> Result<bool, LlmusageError>;
    pub fn set_raw_archive(&self, on: bool) -> Result<(), LlmusageError>;
}
```

新表 `usage_event_raw(event_key TEXT PRIMARY KEY, raw_json TEXT NOT NULL, created_at TEXT NOT NULL)`（migration v7）。状态写 `meta('raw_archive_enabled', '1'|'0')`。解析 driver 在 `commit_shard` 同事务里写 raw 行；开关关闭时跳过。

OpenCode 源（D11）：parser 把 row 字段（model / tokens / 时间戳 / project_*）按列名 → 值 序列化为 JSON 字符串落 raw 表。语义统一为 JSON。

ccr-ui 在 init 阶段调 `bootstrap_with(BootstrapOptions { enable_raw_archive: true })`。关闭走 `set_raw_archive(false)` + `sync --rebuild` 清旧 raw 数据。

### F2 公开 API 表面（库依赖）

#### F2.1 模块路径（stable from 0.5.0）

```text
llmusage::paths::{AppPaths}
llmusage::store::{Store, BootstrapOptions, WorkerLock, HolderKind}
llmusage::query::{Dashboard, QueryFilter, ReportTimezone,
                  OverviewPayload, HomeOverviewPayload,
                  DailyTrendPoint, HeatmapPoint, LogsQuery, LogsPage,
                  DiagnosticsPayload, SourceDiagnostics,
                  ModelBreakdown, SourceBreakdown, ProjectBreakdown,
                  CostBreakdown, CostLine, PricingStatus}
llmusage::sync::{run_with_progress, SyncOptions, SyncEvent, SyncSummary,
                 JobRegistry, JobSnapshot, JobStatus}
llmusage::integrations::{probe_all, install_all, uninstall_all,
                         IntegrationProbe, IntegrationAction}
llmusage::models::{SourceKind, UsageTokens, UsageEvent, ProjectInfo, SessionInfo}
llmusage::error::{LlmusageError, Result}

// feature = "testing"
llmusage::testing::{Fixture, FixtureBuilder, SeedEvent}
```

非公开（保留内部）：`parsers::*` 的具体实现 / `store::sync_writer::*` / `web::*` / `tui::*` / `commands::*`。

#### F2.2 Store stable surface

```rust
impl Store {
    pub fn new(paths: &AppPaths) -> Self;
    pub fn bootstrap(&self) -> Result<(), LlmusageError>;
    pub fn bootstrap_with(&self, opts: BootstrapOptions) -> Result<(), LlmusageError>;
    pub fn open_connection(&self) -> Result<Connection, LlmusageError>;
    pub fn raw_archive_enabled(&self) -> Result<bool, LlmusageError>;
    pub fn set_raw_archive(&self, on: bool) -> Result<(), LlmusageError>;
    pub fn reset_for_source(&self, src: SourceKind) -> Result<(), LlmusageError>;
    pub fn recompute_costs(&self) -> Result<usize, LlmusageError>;
    pub fn mark_source_file_deleted(
        &self, src: SourceKind, path: &Path,
    ) -> Result<(), LlmusageError>;
    pub fn acquire_worker_lock_with(
        &self, timeout: Duration, kind: HolderKind,
    ) -> Result<WorkerLock, LlmusageError>;
}

pub enum HolderKind { Cli, Library, Hook }
```

### F3 Sync（D4, D5, D27）

#### F3.1 SyncOptions / SyncEvent

```rust
pub struct SyncOptions {
    pub source_filter: Option<SourceKind>,
    pub recent_days: Option<u32>,
    pub rebuild: bool,
    pub parallelism: Option<usize>,
}

pub enum SyncEvent {
    Started        { job_id: String, files_total: u64 },
    SourceStarted  { source: SourceKind, files_total: u64 },
    Progress       { source: SourceKind, files_scanned: u64, records_imported: u64,
                     current_file: Option<String> },
    RecentReady    { source: SourceKind },                 // D27 每源独立
    SourceFinished { source: SourceKind, stats: SourceSyncStats },
    Finished       { summary: SyncSummary },
    Failed         { error: String },
    Cancelled,
}
```

`Progress` 至少 200ms 节流一次。

`SourceSyncStats` 同步契约（0.5.3 起）包含 `absent: bool`。该字段表示可选本地真源不存在但同步未失败，
例如 `OPENCODE_HOME` 已存在但 `opencode.db` 缺失时，OpenCode parser 会返回
`absent = true`、`last_error = Some("OpenCode SQLite DB 缺失")`、`events_seen = 0`、
`events_inserted = 0`，并继续走 `SourceFinished` / 成功 summary。旧 JSON 缺少 `absent` 时按
`false` 反序列化；ccr-ui 适配层应优先读取该 typed flag，不再嗅探 `last_error` 文案判断 absent。

#### F3.2 入口

```rust
pub async fn run_with_progress(
    store: &Store,
    options: SyncOptions,
    cancel: CancellationToken,
    sender: mpsc::Sender<SyncEvent>,
) -> Result<SyncSummary, LlmusageError>;
```

取消语义（D5）：`cancel.cancel()` 后 driver 在「下一个 parser 启动前」+「parser 内每个文件启动前」检查 token，已写数据保留，cursor 不回滚。文件级颗粒（多数 < 1s 反应）。

子进程（D24）：`llmusage sync --json-events` 把 `SyncEvent` 以 NDJSON 写 stdout。

#### F3.3 子命令补全

```text
llmusage sync                                     # 全源 + 全量
llmusage sync --source codex|claude|opencode|antigravity
llmusage sync --recent-days 30
llmusage sync --rebuild --source codex            # D20 仅清 codex 一源
llmusage sync --json-events                       # NDJSON
```

`reset_for_source` 单事务删 6 表（usage_event / usage_bucket_30m / source_cursor / source_sync_status / source_file / usage_event_raw）WHERE source=?。`project_dim` 不动（多源共享）。

### F4 查询（D8, D9, D12, D19, D26, D28）

#### F4.1 QueryFilter

```rust
pub struct QueryFilter {
    pub source: Option<SourceKind>,
    pub model: Option<String>,
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
    pub project_hash: Option<String>,
    pub timezone: ReportTimezone,        // default = ReportTimezone::Local（D19）
}

impl Default for QueryFilter {
    fn default() -> Self { Self { timezone: ReportTimezone::Local, ..Self::empty() } }
}
```

存储 UTC，查询 SQL 把 `event_at` 偏移到 timezone 取本地日期；`since/until` 反向偏移成 UTC 边界。

#### F4.2 Dashboard 方法

```rust
impl Dashboard {
    pub fn open(store: &Store) -> Result<Self, LlmusageError>;
    pub fn overview(&self, f: &QueryFilter) -> Result<OverviewPayload>;
    pub fn home_overview(&self, f: &QueryFilter) -> Result<HomeOverviewPayload>;  // D12
    pub fn trends_daily(&self, f: &QueryFilter) -> Result<Vec<DailyTrendPoint>>;
    pub fn heatmap(&self, f: &QueryFilter, days: u32) -> Result<Vec<HeatmapPoint>>;
    pub fn model_breakdown(&self, f: &QueryFilter) -> Result<Vec<ModelBreakdown>>;
    pub fn source_breakdown(&self, f: &QueryFilter) -> Result<Vec<SourceBreakdown>>;
    pub fn project_breakdown(&self, f: &QueryFilter) -> Result<Vec<ProjectBreakdown>>;
    pub fn cost_breakdown(&self, f: &QueryFilter) -> Result<Vec<CostLine>>;
    pub fn logs(&self, q: &LogsQuery) -> Result<LogsPage>;
    pub fn diagnostics(&self) -> Result<DiagnosticsPayload>;
}
```

输出口径（D8/D9/D18）：output_tokens 字段 = `output_tokens + reasoning_output_tokens`（API 合并）；CLI 报告 `--breakdown` 才单独列 reasoning 列。所有 JSON 字段 snake_case。

`HomeOverviewPayload`（D12）：

```rust
pub struct HomeOverviewPayload {
    pub summary: HomeOverviewSummary,
    pub by_platform: BTreeMap<String, HomeOverviewPlatformStats>,  // requests + tokens
    pub series: Vec<HomeOverviewSeriesItem>,
    pub bootstrap: HomeOverviewBootstrap,                          // is_warm + needs_initial_import
    pub archive: UsageArchiveDiagnostics,
    pub last_updated: String,
}
```

ccr-ui 适配层在 Tauri 命令里把 sessions / needs_session_index / empty_reason 自补。

#### F4.3 Logs 分页（D26）

```rust
pub struct LogsQuery {
    pub filter: QueryFilter,
    pub page_size: u32,                  // default 50
    pub cursor: Option<String>,          // base64url(JSON{event_at, event_key})
    pub include_total: bool,
    pub include_raw_json: bool,
}

pub struct LogsPage {
    pub records: Vec<LogRecord>,
    pub next_cursor: Option<String>,
    pub total: Option<i64>,
}
```

游标解码失败 HTTP 返 400。

#### F4.4 Diagnostics（D15, D28）

```rust
pub struct DiagnosticsPayload {
    pub archive_root: String,            // = paths.root_dir 绝对路径
    pub by_source: Vec<SourceDiagnostics>,
    pub recent_failures: Vec<RunRecord>,
}
pub struct SourceDiagnostics {
    pub source: SourceKind,
    pub live_files: u64,
    pub missing_files: u64,
    pub deleted_files: u64,
    pub recent_completed_at: Option<String>,
    pub history_completed_at: Option<String>,
}
```

### F5 source_file 状态机（D15, ADR 0006）

migration v5 加表：

```sql
CREATE TABLE source_file (
  source TEXT NOT NULL,
  file_path TEXT NOT NULL,
  file_size INTEGER,
  file_state TEXT NOT NULL CHECK(file_state IN ('live','missing','deleted_by_user')),
  last_seen_at TEXT NOT NULL,
  PRIMARY KEY (source, file_path)
);
```

转换规则：

```text
扫描看见         → live
扫描没看见 + live → missing
mark_source_file_deleted(...) → deleted_by_user（手动覆盖 live 与 missing）
deleted_by_user 文件被扫描重新看见 → live（用户重新使用了）
```

入口三套（D15）：

```text
Rust:  Store::mark_source_file_deleted(source, path) -> Result<(), LlmusageError>
CLI:   llmusage diagnostics --forget-file <absolute path> [--source <kind>]
HTTP:  POST /api/diagnostics/forget  body={ source, path }
```

### F6 source_sync_status 二阶段游标（D27 配套）

`source_sync_status` 加列 `recent_completed_at TEXT` / `history_completed_at TEXT`（migration v6）。

```text
recent_completed_at  ← 最后一次跑 --recent-days N 全部扫完的 UTC 时间
history_completed_at ← cursor 推进到最早文件起点的 UTC 时间
```

### F7 多实例 + 路径（D3, D13）

`AppPaths::with_root` 让 ccr-ui 指定 `~/.ccr/llmusage/`（自家 db 与 CLI 默认 `~/.llmusage/` 隔离）。WorkerLock 元信息（migration v8）：

```sql
ALTER TABLE worker_lock ADD COLUMN holder_pid INTEGER;
ALTER TABLE worker_lock ADD COLUMN holder_kind TEXT;  -- 'cli' | 'library' | 'hook'
ALTER TABLE worker_lock ADD COLUMN acquired_at TEXT;
```

读端走 SQLite WAL 不持锁，读永远不被阻塞。`acquire_worker_lock_with(timeout, kind)` 默认超时 30s。

### F8 序列化（D18）

所有 lib / CLI JSON / HTTP JSON / 静态 export HTML 内嵌 JSON 全 snake_case。0.5.0 changelog 列出从 0.4.x 起被废弃的 camelCase 字段名，给 jq 用户一次性映射表。0.5.0 终结对 ccusage CLI 的 JSON 字段兼容声明。

### F9 HTTP API（D24）

0.5.0 后 `/api/*` 完整集合：

```text
现有保留：
  GET /api/overview
  GET /api/trends
  GET /api/models
  GET /api/sources
  GET /api/projects
  GET /api/costs
  GET /api/health

0.5.0 新增（8 个）：
  GET  /api/home_overview
  GET  /api/heatmap?source=&days=365
  GET  /api/logs?source=&model=&since=&until=&page_size=&cursor=&include_total=&include_raw=
  GET  /api/diagnostics
  POST /api/diagnostics/forget                     # body={source,path}
  POST /api/jobs                                   # body=SyncOptions, return {job_id, snapshot}
  GET  /api/jobs/{id}                              # JobSnapshot
  POST /api/jobs/{id}/cancel
```

---

## 6. 非功能需求

```text
┌──────────┬─────────────────────────────────────────────────────────┐
│ 维度     │ 要求                                                    │
├──────────┼─────────────────────────────────────────────────────────┤
│ 性能     │ Dashboard::overview < 50ms (10 万 event)                │
│          │ Logs 分页 < 30ms / 页                                   │
│          │ home_overview 单连接全跑完 < 80ms                       │
│ 内存     │ sync 单轮峰值 < 200MB                                   │
│ 并发     │ 多 reader (WAL) + 单 writer                             │
│ 取消     │ < 1.5s 文件边界中断（D5）                               │
│ Migration│ v0 → v10 全跑完 < 5s（10 万 event 库）                  │
│ 兼容     │ Store::bootstrap_with 自动迁 + 迁前备份（D29）          │
│ 可测     │ 公共 API 100% 集成测试覆盖；testing::Fixture 公开（D16）│
│ 隐私     │ raw_json 默认关；hook payload 不出本机                  │
└──────────┴─────────────────────────────────────────────────────────┘
```

---

## 7. 阶段划分（按 Tauri 命令集验收 — D22）

```text
┌──────┬─────────────────────────────┬──────────────────────────────────────┐
│ 阶段 │ llmusage 交付               │ Tauri 命令通车（ccr-ui 适配层视角）  │
├──────┼─────────────────────────────┼──────────────────────────────────────┤
│ M0-  │ F0.1 migration runner       │ —                                    │
│      │ F0.4 with_root / --home/env │                                      │
│      │ F0.5 Cargo lib/bin          │                                      │
│      │ F0.3 LlmusageError 骨架     │                                      │
│      │ F0.2 JobRegistry 雏形       │                                      │
├──────┼─────────────────────────────┼──────────────────────────────────────┤
│ M0   │ F4.1 QueryFilter 提级       │ get_usage_dashboard_v2（基础字段）   │
│      │ F4.2 home_overview 雏形     │ get_home_overview                    │
│      │ F2 公开 API 表面整理        │                                      │
├──────┼─────────────────────────────┼──────────────────────────────────────┤
│ M1   │ F1.2 cache 拆分             │ get_usage_dashboard_v2 完整字段：    │
│      │ F1.3 双价 + pricing 元信息  │   cost_with_cache / cache_savings /  │
│      │ F1.4 event_count            │   pricing_source / pricing_rate      │
│      │ F4.2 trends_daily / heatmap │ get_usage_heatmap                    │
├──────┼─────────────────────────────┼──────────────────────────────────────┤
│ M2   │ F1.5 raw_archive opt-in     │ start_usage_import_job               │
│      │ F4.3 logs 分页              │ get_usage_import_job                 │
│      │ F5  source_file 三态        │ cancel_usage_import_job              │
│      │ F3  RecentReady / 节流      │ get_usage_logs                       │
│      │ F3.3 sync --rebuild --source│ get_usage_archive_diagnostics        │
│      │ F0.2 JobRegistry 完整       │ POST /api/diagnostics/forget         │
├──────┼─────────────────────────────┼──────────────────────────────────────┤
│ M3   │ F1.1 Gemini 完整源 + hook   │ get_usage_dashboard_v2 含 gemini     │
│      │ F8 snake_case 全口径        │   平台数据                           │
│      │ F0.3 thiserror 化全完成     │ 0.5.0 final                          │
│      │ tag 0.5.0                   │                                      │
└──────┴─────────────────────────────┴──────────────────────────────────────┘
```

每个 Tauri 命令配 1 条 llmusage-side 集成测试（用 `testing::Fixture` seed → 调 lib API → 断言 payload 字段）。ccr-ui 端只需 mock 调用就视作通车。

预发布节奏（D2）：

```text
M0/M0- 完成    → 0.5.0-rc.1   ccr-ui 试 dashboard / home_overview
M1 完成        → 0.5.0-rc.2   ccr-ui 试双价 / heatmap
M2 完成        → 0.5.0-rc.3   ccr-ui 试 import job / logs / diagnostics
M3 完成        → 0.5.0        正式
```

---

## 8. 验收标准（M3 完成）

```text
1.  ccr-db 中 usage_repo + cost_tracker 标 #[deprecated] 并删除业务调用
2.  ccr-ui useUsageStore 内所有 getUsageXxxV2 / getHomeOverview /
    *_usage_import_job 命令的 Tauri 实现退化为 llmusage 适配层薄包装
3.  ccr-ui src/types/usage.ts 字段不变；Vue 零改动通过冒烟
4.  llmusage --json-events sync --recent-days 30 子进程模式可用
5.  llmusage 单测 / 集测覆盖：
      - 四源解析（codex / claude / opencode / antigravity）
      - cache 拆分 + 双价 + recompute_costs 重算
      - heatmap / logs 分页 / cursor 编解码
      - source_file 三态机 + mark_source_file_deleted 三入口
      - cancel 在 file 边界 < 1.5s 反应
      - migration v0 → vN 全跑通 + 失败自动回滚
6.  0.4.x → 0.5.0 db 自动迁 + ~/.llmusage/backups/llmusage.db.pre-0.5.0 出现
7.  4 个新 ADR 合入 docs/adr/
8.  CONTEXT.md 8 个新术语合入
```

---

## 9. 已决议事项

合并 v1 §9 + v1.1 grilling D1–D30。所有 30 项决议汇总在 §3 决议表。

---

## 10. 仍待确认（不影响 0.5.0）

```text
1. Gemini OTel 引入        → 0.6.x（D23）
2. project_dim GC          → 0.5.x patch（D20 留余）
3. doctor --gc-projects    → 0.5.x patch
4. ccr-db importer         → 不做（D7）
```

---

## 11. 0.4.x → 0.5.0 升级行为（D29）

```text
1. 用户 cargo install llmusage / brew upgrade
2. 任何 llmusage 命令首次运行
3. bootstrap 检测 meta 表无 schema_version → schema_version = 0
4. cp ~/.llmusage/llmusage.db ~/.llmusage/backups/llmusage.db.pre-0.5.0
5. 顺序跑 migrations v1..v10
6. 写 meta('schema_version', '10')
7. info!("upgraded llmusage db from v0 to v10. backup at backups/llmusage.db.pre-0.5.0")
```

failure 路径：任意 migration step 失败 → 整事务回滚 + 备份保留 + LlmusageError::MigrationFailed{version,...}。用户可手动回退 0.4.x 二进制 + cp backup 回主路径。

---

## 12. 不在范围内

- 不替代 ccr-ui SessionIndexer / 会话归档功能。
- 不改 ccr-ui Profile / Auth / VS Code 扩展。
- 不引入云端上传 / 远程聚合。
- 不要求 llmusage Web 仪表盘 i18n 与 ccr-ui 对齐。
- v1 不引入 OpenTelemetry（D23）。
- v1 不主动联网刷价目（D21）。
- v1 不写 ccr-db importer（D7）。

---

## 附录 A：ccr-ui 适配层最小落地

M0 完成后 ccr-ui 在 `ccr-ui/src-tauri/src/llmusage_adapter.rs` 大致这样接：

```rust
use llmusage::{
    error::LlmusageError,
    paths::AppPaths,
    store::{Store, BootstrapOptions, HolderKind},
    query::{Dashboard, QueryFilter, ReportTimezone},
    sync::{JobRegistry, SyncOptions},
};

pub struct LlmusageHandle {
    pub store: Store,
    pub jobs:  JobRegistry,
}

impl LlmusageHandle {
    pub fn init(ccr_root: Option<PathBuf>) -> Result<Self, LlmusageError> {
        let paths = match ccr_root {
            Some(p) => AppPaths::with_root(p)?,
            None    => AppPaths::discover()?,
        };
        let store = Store::new(&paths);
        store.bootstrap_with(BootstrapOptions { enable_raw_archive: true })?;
        Ok(Self { store, jobs: JobRegistry::default() })
    }

    pub fn dashboard(&self, filter: QueryFilter) -> Result<serde_json::Value, LlmusageError> {
        let dash = Dashboard::open(&self.store)?;
        Ok(serde_json::json!({
            "summary":       dash.overview(&filter)?,
            "trends":        dash.trends_daily(&filter)?,
            "model_stats":   dash.model_breakdown(&filter)?,
            "project_stats": dash.project_breakdown(&filter)?,
            "heatmap":       dash.heatmap(&filter, 365)?,
            "archive":       dash.diagnostics()?,
            "generated_at":  chrono::Utc::now().to_rfc3339(),
        }))
    }
}

#[tauri::command]
pub async fn get_usage_dashboard_v2(
    state: State<'_, AppState>,
    platform: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<serde_json::Value, String> {
    let filter = QueryFilter {
        source:   platform.as_deref().map(parse_source).transpose().map_err(stringify)?,
        since:    start_date.as_deref().map(parse_naive_date).transpose().map_err(stringify)?,
        until:    end_date.as_deref().map(parse_naive_date).transpose().map_err(stringify)?,
        timezone: ReportTimezone::Local,
        ..Default::default()
    };
    state.llmusage.dashboard(filter).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_usage_import_job(
    state: State<'_, AppState>,
    recent_days: Option<u32>,
    rebuild: bool,
    source: Option<String>,
) -> Result<JobSnapshot, String> {
    let opts = SyncOptions {
        source_filter: source.as_deref().map(parse_source).transpose().map_err(stringify)?,
        recent_days,
        rebuild,
        parallelism: None,
    };
    let (id, _rx) = state.llmusage.jobs.start(&state.llmusage.store, opts)
        .map_err(|e| e.to_string())?;
    Ok(state.llmusage.jobs.snapshot(&id).expect("just created"))
}
```

ccr-ui 的 Vue 与 `src/types/usage.ts` 不需要改动 — Tauri 层处理 sessions 字段补齐与 `empty_reason` 推断。
