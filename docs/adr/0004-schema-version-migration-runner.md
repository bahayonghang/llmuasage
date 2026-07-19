# ADR 0004 — schema_version + 自家 versioned migration runner

- 状态：拟稿（0.5.0 sprint M0- 落地）
- 落地阶段：M0- 落 runner + v1 baseline；M1/M2/M3 随功能追加 v2-v10；0.6.x 追加 v11 行为事实表；v12 修复 `source_sync_status` 历史列漂移；M0- 不单独发布 rc
- 落地日期：TBD
- 相关代码：`src/store/schema.rs`、`src/store/migrations.rs`（新）、`src/store/mod.rs::bootstrap`
- 相关术语：Migration / SchemaVersion / Store（见仓库根目录 CONTEXT.md）
- 关联 PRD：llmusage-integration-prd-v1.1.md §F0.1（D1，仓库根目录）

## 背景

0.4.x 通过 `CREATE TABLE IF NOT EXISTS` + `ensure_column` 探测式 ALTER 维护 schema。这套办法可加列，但表达不出：

- rename / split 列（如 `cached_input_tokens` → `cache_read_tokens` + `cache_creation_tokens`）
- 删除旧表
- 数据回填式迁移（如 0.5.0 的 `cost_with_cache_usd` 全量 backfill）
- "不许从新版回退到旧版"的版本号断言

0.5.0 引入 6+ 张表 / 列的结构变更，必须先把 schema 升级机制本身搞稳。M0- 只落 runner 与 v1 baseline，后续版本号随真实功能 migration 逐步追加；禁止用空 migration 在 M0- 预占 v2-v10。

## 决策

### 1. 引入 `meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)` 表

固定行 `meta('schema_version', 'N')`。读不到时视为 v0（即 0.4.x 老库）。

### 2. `MIGRATIONS: &[(u32, &str, fn(&Transaction) -> Result<()>)]`

编译期常量数组，按版本号升序。每个 migration 是 `fn(&Transaction)`，不允许跨步引用其他 migration 的内部函数。数组可随阶段增长：M0- 只有 v1 baseline；M1 追加 v2-v4；M2 追加 v5-v7；M3 追加 v8-v10；0.6.x 追加 v11，兼容修复追加 v12。

```rust
const MIGRATIONS: &[(u32, &str, MigFn)] = &[
    (1,  "baseline",            m_001_baseline),
    (2,  "add_cache_split",     m_002_cache_split),
    (3,  "add_cost_breakdown",  m_003_cost_breakdown),
    (4,  "add_event_count_proj",m_004_event_count_proj),
    (5,  "add_source_file",     m_005_source_file),
    (6,  "add_recent_history",  m_006_recent_history),
    (7,  "add_raw_archive",     m_007_raw_archive),
    (8,  "add_worker_lock_meta",m_008_worker_lock_meta),
    (9,  "add_gemini",          m_009_gemini),
    (10, "add_pricing_meta",    m_010_pricing_meta),
    (11, "add_behavior_facts",   m_011_behavior_facts),
    (12, "repair_source_sync_status_history_columns",
                                      m_012_repair_source_sync_status_history_columns),
];
```

### 3. baseline (v1) 必须 idempotent

老库（0.4.x）的所有表已存在；baseline 跑 `CREATE TABLE IF NOT EXISTS` + `ensure_column` 让"全新 install" 与"老库升级"在 v1 边界后等价。

### 4. 升级前自动备份

检测到 `schema_version = 0` 时，`bootstrap()` 在跑 v1 之前 `cp db_path → backups/llmusage.db.pre-0.5.0`。一次升级一份备份，不覆盖。

### 5. 单事务包一步

每个 migration 在独立事务内：

```text
BEGIN IMMEDIATE
fn(tx)?
UPDATE meta SET value=N WHERE key='schema_version'
COMMIT
```

任意一步失败 → 整事务回滚 + 返回 `LlmusageError::MigrationFailed { version, source }`，备份保留。

### 6. migration 进度是观测通道，不是持久状态

`run_migrations_with_events` 可选接收 `MigrationProgressEvent` sink。每步在 `BEGIN IMMEDIATE` 前发 started，commit 成功后发 finished + `elapsed_ms`，并同步写 tracing 日志。CLI 将这些事件转成默认 stderr 阶段提示或 `sync --json-events` NDJSON；migration 本身不把进度写入 SQLite，失败回滚语义和 `schema_version` 推进规则不变。

### 7. v4 event_count 回填必须一次性聚合

`m_004_add_event_count_proj` 禁止逐 bucket 相关子查询扫描 `usage_event`。回填策略是：

1. 建 `temp.llmusage_event_count_backfill`。
2. `INSERT ... SELECT source, model, hour_start, COALESCE(project_hash, ''), COUNT(*) FROM usage_event GROUP BY ...` 一次性聚合。
3. 按 `usage_bucket_30m` 主键从临时表查回 `event_count`。

这保持 v4 在旧库大表上接近 `usage_event + bucket` 线性复杂度，避免 0.5.0 首次升级时 `bucket_count × event_count` 卡住。

## 备选方案与否决理由

### 备选 A：refinery crate

成熟，支持 SQL 文件扫描。否决：

1. 编译期依赖增加（rusqlite 已是 bundled，refinery 引入 tokio-postgres 兼容 trait）。
2. 二进制变大约 200KB。
3. SQL 文件扫描对单二进制 + embed 资源风格的 llmusage 不顺手。
4. 30 行手写代码就能覆盖。

### 备选 B：barrel crate

DSL schema builder。否决：DSL 学习成本高，团队习惯手写 SQL。

### 备选 C：保留探测式 + 加 schema_version 单字段

否决：探测式无法表达 rename / split / drop。0.5.0 的 `cached_input_tokens → cache_read_tokens` 必须用 ALTER + UPDATE backfill 才能保留数据。

## Deletion-test 论证

如果删除 `src/store/migrations.rs` 与 `meta` 表 → bootstrap 退化为 0.4.x 探测式 → 0.5.0 任何 cache_split / cost_breakdown 列都无法 backfill 老数据 → 0.4.x 升级用户的历史 cost 永远是 0。

→ migration runner 是必需机制，不可删除。

## 后果

正面：

- schema 演进可表达任意结构变化（rename / split / drop / backfill）。
- 0.4.x → 0.5.0 用户感知零成本（自动迁 + 自动备份）。
- 后续阶段和 0.6.x 加新版本只需在 `MIGRATIONS` 数组追加一行；追加项必须对应真实 schema/data 变更，不允许空占位。

负面：

- baseline (v1) 必须严格 idempotent，否则全新 install 与升级路径会分叉。CI 必须有"在 v1 跑两遍"的回归测试。
- 失败回滚后用户必须手动恢复（无 down migration）。文档需明确"down migration 故意不实现"。

## 0.6.x 更新：v11 行为事实表

0.6.x 为 dashboard Activity / Tools / Optimize / Compare 增加 migration v11：

- `usage_turn`：turn-level normalized 行为事实，保存 source/session/path/model/category/one-shot/retry/token 汇总等字段。
- `usage_tool_call`：tool/action-level normalized 行为事实，保存 tool kind、MCP server/tool、safe preview、input fingerprint 等字段。
- v11 只追加行为分析事实表和索引，不改变 `usage_event` / `usage_bucket_30m` 的成本与用量主路径语义。
- `SyncShard` 和 `SyncRunWriter::commit_shard` 负责将 parser 提取的行为事实与同一 `source_path_hash` 的 reset 保持幂等。

这延续本 ADR 的核心约束：所有 schema 变更仍通过 `MIGRATIONS` 追加真实版本号，不使用空 migration 占位。

## 2026-05-17 更新：v12 `source_sync_status` 兼容修复

真实用户库出现 `meta('schema_version') == 11`，但物理表 `source_sync_status` 缺少 `stored_events` 列的漂移状态。原因是 `stored_events` 曾作为 v6 `add_recent_history` 的幂等 `ensure_column` 追加；如果某个历史构建已经把库推进到 v6+ / v11，却没有该列，当前 runner 会跳过所有已完成版本，后续 `SyncStatusStore::save_source_sync_statuses` 无条件写入 `stored_events` 时触发 SQLite `no column named stored_events`。

v12 是真实兼容修复 migration：重新以幂等方式确保 `recent_completed_at`、`history_completed_at` 与 `stored_events` 三个 `source_sync_status` 历史列存在，并将 `schema_version` 推进到 12。该修复不重建表、不删除数据、不改变 `usage_event` / `usage_bucket_30m` 语义。

## 验证

- M0- 单测：`migration_runner_runs_in_order_from_v0_to_v1_baseline`
- M0- 单测：`migration_runner_idempotent_when_already_at_latest`
- M0- 单测：`migration_failure_rolls_back_transaction_and_keeps_backup`
- M0- 集测：用 0.4.x 测试库 fixture 跑 bootstrap，断言：
  - schema_version == 1
  - backups/llmusage.db.pre-0.5.0 存在
  - 0.4.x 既有 usage_event 行未丢
  - v1 baseline 表结构与现 0.4.1 `Store::bootstrap()` 输出一致
- M3 final 集测：用 0.4.x 测试库 fixture 跑 0.5.0 final bootstrap，断言：
  - schema_version == 10
  - backups/llmusage.db.pre-0.5.0 存在
  - usage_event 既有行的 cache_read_tokens 等于原 cached_input_tokens
  - cost_with_cache_usd 被 backfill（不是 0）
- 0.6.x 行为事实表单测：`migration_v11_creates_behavior_fact_tables`，断言 `usage_turn` / `usage_tool_call` 存在且 `schema_version == 11`。
- v12 兼容修复单测：`migration_v12_repairs_source_sync_status_columns_on_drifted_v11_db`，断言漂移 v11 库升级后 `stored_events` 存在、既有行默认值为 0，且 `schema_version == 12`。

## 2026-07-19 更新：v15 来源增量游标与行为 reset 索引

v15 `optimize_source_sync_cursors_and_behavior_resets` 是真实 schema migration：

- `source_cursor.last_part_rowid INTEGER NOT NULL DEFAULT 0` 持久化 OpenCode `part.rowid` 高水位；旧库升级后执行一次幂等工具事实 backfill。
- `idx_usage_turn_source_path_hash(source, source_path_hash)` 与 `idx_usage_tool_call_source_path_hash(source, source_path_hash)` 支撑 `SyncShard` 按来源文件 reset 行为事实。
- migration 不修改 OpenCode 自有数据库，也不回填伪造的 rowid；列默认值和两个 `CREATE INDEX IF NOT EXISTS` 保持 fresh/upgrade 幂等。与 v13/v14 一致，极端漂移库缺少目标表时该项 no-op，不在后续 migration 中凭空重建旧 schema。

验证：`migration_v15_adds_opencode_part_cursor_and_behavior_reset_indexes` 同时断言 schema version、列默认值、索引存在性以及 reset 查询计划使用对应复合索引。
