

我先读 PRD 确认需求，然后系统性检查 llmusage 各模块对照 §3/§4 的字段级 GAP。



骨架基本对齐 PRD。继续深入检查 store/migrations、sync 实现、query 接口、以及 Cargo/CHANGELOG/integrations。

继续核对：sync 事件枚举、Dashboard 方法、QueryFilter、commands::sync、integrations::gemini、parsers::gemini、source_file 状态机、JSON 事件 CLI。

继续核对。剩下要查：CLI 子命令的 `--source/--recent-days/--rebuild/--json-events/--home`、integrations/gemini、parsers/gemini、`source_file` 状态机、migrations 升级链、pricing catalog、worker_lock 等。





确认 ccr-ui 的 TypeScript 契约里 [LogRecord](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/logs.rs:43:0-87:1)、`ModelStat`、`DailyTrend` 各字段，以便给出"是否真的影响 ccr-ui 集成"的准确判断。




已对照 PRD 全部检查完。下面给出结论。

# llmusage 0.5.0 改造合规性审查

**仓库**：`origin https://github.com/bahayonghang/llmuasage.git`（注意：远端仓库名拼成 `llmuasage`，与 `Cargo.toml.repository` 一致；如果是 typo 建议同步改名）。本地 `main` 与 `origin/main` 完全同步，最新 2 笔提交即为 0.5 集成包：`1a07591 feat(core): ✨ 打通 0.5 本地集成面` + `165d643 docs(release): 📝 记录 0.5 集成与迁移说明`。

## 1. 已完整落地（覆盖 PRD §F1–§F8 主线）

- **Cargo 元信息**：`@D:/Documents/Code/CLI/llmusage/Cargo.toml:12-18` 显式声明 `[lib]` + `[[bin]]`，version 已升 `0.5.0`，[testing](cci:9://file:///D:/Documents/Code/CLI/llmusage/src/testing:0:0-0:0) feature 已加。
- **错误层**：`@D:/Documents/Code/CLI/llmusage/src/error.rs:9-80` 用 thiserror 派生 `LlmusageError`（`#[non_exhaustive]`），含 `Io / Db / NotInitialized / MigrationFailed / Parse / LockBusy / ConfigInvalid / PricingMissing` 8 个 variant，全有单测。`Result<T, LlmusageError>` 别名也在 [lib.rs](cci:7://file:///D:/Documents/Code/CLI/llmusage/src/lib.rs:0:0-0:0) 公开。
- **路径层**：`@D:/Documents/Code/CLI/llmusage/src/paths.rs:29-64` [AppPaths::discover](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/paths.rs:26:4-33:5) / [with_root](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/paths.rs:35:4-41:5) / [with_cli_home](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/paths.rs:43:4-63:5) 三入口齐全；`LLMUSAGE_HOME` + 全局 `--home <PATH>` flag（`@D:/Documents/Code/CLI/llmusage/src/commands/mod.rs:32-39`）已接通。
- **Gemini 源**：`SourceKind::Gemini` 加入 [models.rs](cci:7://file:///D:/Documents/Code/CLI/llmusage/src/models.rs:0:0-0:0)，`sources::registered_parsers / registered_integrations` 注册了 `GeminiParser` 与 [GeminiIntegration](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/integrations/gemini.rs:19:0-19:29)；hook 安装走 `~/.gemini/settings.json::hooks.SessionEnd`，含备份/idempotent/uninstall 测试（`@D:/Documents/Code/CLI/llmusage/src/integrations/gemini.rs:233-287`）。
- **数据模型拆分（F1.2-F1.4）**：
  - [UsageTokens](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/models.rs:50:0-69:1) 拆 `cache_read_tokens` + `cache_creation_tokens`，并保留 `serde alias = "cached_input_tokens"` 兼容旧 JSON。
  - migration v2 把 `usage_event` / `usage_bucket_30m` 的 `cached_input_tokens` 重命名为 `cache_read_tokens`，新增 `cache_creation_tokens INTEGER NOT NULL DEFAULT 0`。
  - migration v3 把 `cost_with_cache_usd / cost_without_cache_usd / pricing_status / pricing_source / pricing_rate` 加到两表。
  - migration v4 把 `event_count` 加到 bucket 表并用 temp 聚合表回填，避免 PRD 担忧的 per-bucket correlated COUNT(*)。
  - migration v6/v7/v8/v9/v10 分别落地 `recent_completed_at/history_completed_at`、`usage_event_raw`、`worker_lock` 元数据、Gemini meta、`pricing_catalog_version` meta。
- **Bootstrap opt-in（F1.5）**：`BootstrapOptions { enable_raw_archive: Option<bool> }`（PRD 是 `bool`，这里实现成 `Option<bool>` —— 等价或更优，None 时保留旧值）；[Store::raw_archive_enabled()](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/store/mod.rs:368:4-375:5) / `set_raw_archive` 走 [meta](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/store/lock.rs:23:4-26:5) 表持久化。`SyncShard.raw_records` 与 [SyncRunWriter.raw_archive_enabled()](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/store/mod.rs:368:4-375:5) 配合，确保单次 sync 要么全写 raw 要么全不写。
- **Worker 锁（§9.4）**：`WorkerLockMeta { holder_pid, holder_kind, acquired_at, lease_expires_at, updated_at }` + [acquire_worker_lock_with(timeout, HolderKind::{Cli|Library|Hook})](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/store/lock.rs:45:4-72:5) + `LlmusageError::LockBusy { holder }` 完整闭环；hook 路径仍用旧的非阻塞 `try_acquire`（PRD 期望，正确）。
- **Sync 任务化（F3）**：`@D:/Documents/Code/CLI/llmusage/src/sync/job_registry.rs` 提供 `JobRegistry::start / snapshot / cancel / list_recent`，事件类型 `parsers::SyncEvent` 含 `Started / SourceStarted / Progress / RecentReady / SourceFinished / Finished / Failed / Cancelled / LockWaiting / LockAcquired / Migration*` 十余个 variant，覆盖 PRD §F3.1 全部要求。`commands::sync` 子命令补全 `--source / --recent-days / --rebuild / --json-events` 全部到位（`@D:/Documents/Code/CLI/llmusage/src/commands/mod.rs:55-69`）；`--rebuild --source <X>` 会调 `Store::reset_for_source(SourceKind)` 仅清单源，符合 §9.5。
- **查询（F4）**：`Dashboard::open / overview / trends_daily / model_breakdown / source_breakdown / project_breakdown / cost_breakdown / heatmap / logs / diagnostics / health / home_overview / snapshot` 全部接 `&QueryFilter`；老 [trends(window)](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:336:4-384:5) 标 `#[deprecated]` 保留兼容。`QueryFilter { source, model, since, until, project_hash, timezone }` 与 `ReportTimezone::Utc/Local/Fixed` 一一对齐 PRD §F4.1。
- **logs 分页**：`LogsQuery { filter, page_size, cursor, include_total, include_raw_json }` + base64url(JSON{event_at,event_key}) 游标 + `include_raw_json` 关联 `usage_event_raw` LEFT JOIN，与 PRD §F4.4 字面一致。
- **heatmap**：`HeatmapPoint { date, event_count, total_tokens }`，1..=366 days clamp，时区折算正确。
- **source_file 状态机（F5.1）**：`source_file` 表 + 5 条状态转移单测（live↔missing↔deleted_by_user）+ [Store::mark_source_file_deleted](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/store/source_file.rs:184:4-211:5) 原子清 cursor + 受 ADR 0006 规约。
- **Diagnostics（F5.3）**：`DiagnosticsPayload { archive_root, by_source, recent_failures }` + `SourceDiagnostics { live_files, missing_files, deleted_files, recent_completed_at, history_completed_at }`，HTTP `/api/diagnostics` 镜像 + `diagnostics --forget-file` CLI 入口。
- **价目表生命周期（§9.6）**：`pricing/static-v1.json` 编译期 embed + [PricingCatalog::load_snapshot](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/query/pricing_catalog.rs:59:4-74:5) 读本地 JSON + `doctor --refresh-pricing <PATH>` 拒绝 URL，`PricingStatus::Static/Snapshot/Unpriced` 三态 + `pricing_source` 取值 `"static-v1" | "<file.version>" | None`。
- **集成入口**：`integrations::probe_all / install_all / uninstall_all / IntegrationProbe / IntegrationAction` 公开。
- **Testing 钩子**：[testing](cci:9://file:///D:/Documents/Code/CLI/llmusage/src/testing:0:0-0:0) feature 后的 `Fixture` 已暴露在 [lib.rs](cci:7://file:///D:/Documents/Code/CLI/llmusage/src/lib.rs:0:0-0:0) 让 ccr-ui 写 e2e。
- **序列化**：library 与 CLI JSON 都统一切到 snake_case（CHANGELOG 0.5.0 已声明）。

## 2. 与 PRD 字面有差异（不阻塞集成，但 ccr-ui 落地时会撞到）

下表是 ccr-ui `@d:/Documents/Code/Github/ccr/ccr-ui/src/types/usage.ts:1-95` 期望、与 llmusage 当前公开结构的字段差距。**列已在 SQLite 落库**，缺的只是 Dashboard 查询的 SELECT 没读出来：

- **[OverviewPayload](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:62:0-81:1)** 缺 `total_cost_usd` 与 [cache_efficiency](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:49:4-57:5) 字段输出。[TokenSummary::cache_efficiency()](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:49:4-57:5) 仅作方法，[UsageSummary](cci:2://file:///d:/Documents/Code/Github/ccr/ccr-ui/src/types/usage.ts:3:0-10:1) 期望直接拿到。
- **[DailyTrendPoint](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:97:0-112:1)** 缺 `cost_with_cache_usd: f64`（PRD §F4.2 明文要求）。bucket 表已有列，SQL 加一行 `SUM(cost_with_cache_usd)` 即可。
- **[ModelBreakdown](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:116:0-131:1)** 缺 `cost_with_cache / cost_without_cache / cache_savings / pricing_status / pricing_source / pricing_rate`（PRD §3 字段级 GAP 表里 F1 类必须由 llmusage 提供）。当前只能拿 `CostLine.estimated_cost_usd` 单值。
- **[ProjectBreakdown](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:148:0-159:1)** 缺 `project_path` 与 `total_cost`。`usage_event.project_path` 列已写库（migration v4），bucket 维度上没有但 event 表可 join。
- **[LogRecord](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/logs.rs:43:0-87:1)** 缺 `recorded_at`（对应 `usage_event.created_at`）、`cost_usd / cost_with_cache_usd / cost_without_cache_usd / pricing_status / pricing_source`、以及 `id`（ccr-ui 习惯用独立 id，目前可由 `event_key` 兜底）。`record_json` 字段名前端期望，llmusage 出 `raw_json`，需要 Tauri 适配层做名字翻译。

## 3. PRD 草图与实现风格差异（属于实现选择，非缺陷）

- **[SyncOptions](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/sync/job_registry.rs:71:0-78:1) 字段命名**：PRD §F3.1 写 `source_filter: Option<SourceKind>` + `parallelism: Option<usize>`；`@D:/Documents/Code/CLI/llmusage/src/sync/job_registry.rs:70-79` 实际为 `{ rebuild, recent_days, source: Option<String> }`，无 `parallelism`。[source](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/integrations/gemini.rs:22:4-24:5) 用 `String` 让 Tauri 适配多了一道字符串解析；`parallelism` 缺失意味着 ccr-ui 没法外部限制并发（默认走 `min(available_parallelism, 4)`）。
- **同步入口形态**：PRD §F3.2 草图是 `pub async fn sync::run_with_progress(store, options, cancel, sender) -> Result<SyncSummary>`；当前是 [JobRegistry::start(store, options) -> (JobId, mpsc::Receiver<JobEvent>)](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/sync/job_registry.rs:81:4-109:5) + [JobRegistry::cancel(&id)](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/sync/job_registry.rs:118:4-130:5)。功能等价但风格不同——PRD 附录 A 的草图代码（`tokio::spawn(llmusage::sync::run_with_progress(...))`）无法字面复用。[commands::sync::run_once_with_cancel](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/commands/sync.rs:294:0-371:1) 实际具备所需签名，可以加 1 行 `pub use` 或薄包装就字面对齐。
- **CLI JSON 全切 snake_case**：PRD §F8 建议两套（CLI 保留 camelCase 兼容 ccusage、library 输出 snake_case）；CHANGELOG 直接合一到 snake_case 并给了 jq 迁移表。这是有意为之，破坏 ccusage 兼容但简化了一切，对 ccr-ui 集成无影响。
- **`Cargo.toml.repository` URL**：拼成 `llmuasage`，与 GitHub 远端 typo 一致。如属有意保留，OK；否则建议同步改名。

## 4. 落地建议（按优先级）

- **高（直接影响 ccr-ui 适配工作量）**
  - **[DailyTrendPoint](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:97:0-112:1) 加 `cost_with_cache_usd`**：单行 SQL，立即解锁前端"每日成本曲线"。
  - **[ModelBreakdown](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:116:0-131:1) 加双价 + pricing 元数据**：把 `usage_bucket_30m.cost_with_cache_usd / cost_without_cache_usd / pricing_status / pricing_source / pricing_rate` 暴露出来，并派生 `cache_savings = cost_without - cost_with`。这是 PRD §3 明确要求的 F1 类。
  - **[LogRecord](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/logs.rs:43:0-87:1) 加 `recorded_at`（=`created_at`）+ cost/pricing 5 字段**：列已存在，只需 SELECT 加 6 列。
  - **[OverviewPayload](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:62:0-81:1) 加 `total_cost_usd` + [cache_efficiency](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:49:4-57:5)**：避免前端各家自己除一遍。
- **中（API 风格对齐）**
  - 加 `pub async fn sync::run_with_progress(...)` 薄封装，字面对齐 PRD 草图，方便 ccr-ui 直接复制 §6 / 附录 A 示例。
  - [SyncOptions::source](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/integrations/gemini.rs:22:4-24:5) 改 `Option<SourceKind>`、补 `parallelism: Option<usize>`。
  - [ProjectBreakdown](cci:2://file:///D:/Documents/Code/CLI/llmusage/src/query/mod.rs:148:0-159:1) 加 `project_path` 与 `total_cost_usd`（如果 ccr-ui 真要 path 去重；目前只暴露 hash + label）。
- **低（修文档/元信息）**
  - `Cargo.toml.repository` 与 GitHub 仓库名 typo（`llmuasage` → [llmusage](cci:9://file:///D:/Documents/Code/CLI/llmusage:0:0-0:0)）。
  - 把"Diagnostics live/missing/deleted 求和"约定写进 README，省 ccr-ui Tauri 适配层的来回确认。

## 5. 总评

**核心改造已经达到 M3 / v1 验收**：四源（含 Gemini）、双价、cache 拆分、event_count、raw archive、source_file 状态机、recent/history 完成时间、cancel + RecentReady、cursor 分页 logs、heatmap、[with_root](cci:1://file:///D:/Documents/Code/CLI/llmusage/src/paths.rs:35:4-41:5)、worker_lock 元数据、thiserror 错误层、ADR、CHANGELOG、迁移测试都到位。**剩余 7-8 个字段级缺口集中在 `Dashboard::*` 的 SELECT 输出**——列都已经在 SQLite，补一轮"暴露查询字段"即可让 ccr-ui Vue 契约 100% 零改动通过冒烟。建议在合并 ccr-ui 适配 PR 之前先在 llmusage 走一个 0.5.1 patch 把 §2 高优先级那 4 处补齐，可以省掉 ccr-ui Tauri 层一半的"字段映射 + 二次 SUM"代码。