# ADR 0004 — schema_version + 自家 versioned migration runner

- 状态：拟稿（0.5.0 sprint M0- 落地）
- 落地阶段：M0- 落 runner + v1 baseline；M1/M2/M3 随功能追加 v2-v10；0.6.x 追加 v11 行为事实表；v12 修复 `source_sync_status` 历史列漂移；v18 修复 Behavior 查询索引；v19 增加 Activity 成本覆盖索引；M0- 不单独发布 rc
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

## 2026-07-26 更新：v17 有界 JSONL 问题诊断

v17 `add_source_sync_parse_issues` 在 `source_sync_status` 增加
`parse_issues_json TEXT NOT NULL`，默认值是零计数和空样本。该列持久化最近一次
source sync 的 malformed/oversized 计数与最多 8 条安全样本；样本只包含 source、
path hash、record offset 和 issue kind，不包含原始 JSONL、prompt、assistant 内容或
完整路径。

迁移只使用幂等 `ensure_column`，不重建表、不改变 usage/cursor/token accounting
语义。验证由 `migration_v17_adds_bounded_parse_issue_diagnostics` 和
`parse_issue_diagnostics_round_trip_and_reject_invalid_json` 覆盖。

## 2026-07-28 更新：v18 Behavior 查询索引

v18 `optimize_behavior_query_indexes` 为 Activity、Tools、Optimize、Compare
的有界时间投影、session cost lookup、event/tool attribution 和 selected-model
tool count 增加索引，并重新创建历史 v11 已声明但部分既有库缺失的
`idx_usage_turn_event_key_expr`。迁移只创建索引，不重写 usage facts；fresh schema
与漂移的 schema-v17 数据库必须得到相同的索引集合。

schema_version 升到 18 后，旧二进制会按本 ADR 的 newer-schema 约束拒绝打开。
因此对真实既有数据库执行首次 v18 bootstrap 前必须创建并验证独立的 SQLite
online backup。回滚方式是恢复该备份或继续使用支持 v18 的二进制；仅删除索引
不是版本回滚。验证由
`migration_v18_repairs_v17_index_drift_and_matches_fresh_schema` 和 Behavior 查询计划
测试覆盖。

## 2026-07-30 更新：v19 Activity 成本覆盖索引

v19 `optimize_activity_event_cost_projection` 只创建
`idx_usage_event_activity_cost`：

```sql
CREATE INDEX IF NOT EXISTS idx_usage_event_activity_cost
    ON usage_event(event_key, cost_with_cache_usd);
```

该索引把 Activity 的全量 `event_key + cost_with_cache_usd` 投影从表扫描变为 covering
index scan。migration 不修改 Activity SQL、Rust reducer、filter、cache、并发、前端、
PERF-002 生命周期或 3 秒截止线，也不执行数据回填。schema-v18 升级与 fresh bootstrap
必须得到相同的两列索引；v18 自身的测试固定使用 `MIGRATIONS[..18]`，避免 latest schema
推进后混入 v19 证据。

接受 v19 还需要两项 migration 以外的门：Activity 在建索引前后对 missing event、
NULL cost、`edit_turns=0`、category 并列和各类 filter 的序列化字节完全一致；固定
4,000-event `SyncRunWriter` 合成基准的七轮中位数回归不得超过 10%。任何一项失败都
回滚 v19，不自动进入 D2 查询设计。

真实数据库首次升级前仍须保留并验证独立 pre-v19 online backup。旧二进制会按
newer-schema 约束拒绝 v19；回滚方式是恢复该备份，而不是仅删除索引或手工下调
`schema_version`。最终冷启动验收使用五份由固定 binary 准备且尚未发起 Activity 请求
的 v19 快照，并在人工重启后消费。

验证由 `migration_v19_upgrades_v18_and_matches_fresh_schema`、
`activity_serialization_is_identical_before_and_after_v19_index`、显式单线程
`activity_cost_index_sync_throughput_regression_stays_within_ten_percent` 及重启后首次触库
矩阵覆盖。

## 2026-08-23 更新：v24 Top Sessions covering expression index

v24 `optimize_top_sessions_identity_order` 只创建一个真实索引，不新增列、表或 backfill：

```text
idx_usage_event_top_sessions_cover(
  <session_identity_sql("") 的完整 CASE 表达式>, event_at,
  session_label, project_label, source,
  total_tokens, output_tokens, reasoning_output_tokens,
  cost_with_cache_usd, model, project_hash, host_id
)
```

完整 CASE 仍按非空 `session_id`、`source_path_hash`、Codex/Claude event-key session、
完整 event key 的顺序回退，并保留 source 前缀。migration 内先
`DROP INDEX IF EXISTS idx_usage_event_top_sessions_cover`，再以 canonical SQL 创建；两步与
schema version 写入位于同一 immediate transaction。因此 v23 没有索引、同名 drifted
索引和同名 exact 索引都会收敛到同一定义；创建失败则恢复 transaction 前的索引与 v23
version。query-side normalized parity test 将 index expression 与 `session_identity_sql("")`
绑定，避免两份表达式静默漂移。

无界 Top Sessions 投影显式使用该 covering index，避免 source/host filter 被 planner 送到
旧非覆盖索引；出现任一日期边界时不加 hint，继续让 `event_at` 范围索引参与选择。采用 v24
前必须同时通过：pre-v24 legacy 完整 JSON 逐字节等价、目标 query plan、代表库索引分配
不超过 15%、固定七轮交替 4,000-event `SyncRunWriter` 中位数回归不超过 10%，以及一次
warm-up + 五次顺序样本的完整 HTTP 矩阵。

错误与回滚矩阵：

| 输入状态 | 结果 |
| --- | --- |
| fresh / v23 无同名索引 | 创建 canonical index，version=24 |
| v23 有 drifted 或 exact 同名索引 | transaction 内替换为 canonical index，version=24 |
| index 创建失败 | 回滚旧索引与 schema v23 |
| v23 binary 打开 v24 DB | newer-schema 拒绝 |

真实数据库首次升级前必须创建并验证独立 SQLite online backup。旧二进制不能作为已迁移库
的原地回滚；回滚只能恢复 verified pre-v24 backup，或继续使用支持 v24 的 binary。不得只
删除索引或手工下调 `schema_version`。本任务的 migration/performance 验收仅在 task-owned
副本执行，活动数据库未迁移；未跨真实系统重启，cold/first-touch 保持 `UNVERIFIED`。

验证由五类 v24 migration tests、Top Sessions pre-v24 serialized oracle、query-plan/parity
tests、显式 ignored 写入门禁和代表性 HTTP matrix 覆盖。专用
`scripts/benchmark-top-sessions.mjs` 固定 24-case / 120-sample 协议；`/api/sessions` 通过
`Server-Timing: sessions-query;dur=<ms>` 提供逐样本服务端 query pipeline timing，输出
allowlist 禁止保留 URL、响应 rows 与实际 filter 值。
