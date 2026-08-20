# C1 技术设计

设计依据：父任务 `design.md` 第 2.1、3.1、3.2、3.3、3.4、5.1、7 节。本文件只记录父设计未覆盖的子任务级细节。

## 迁移编号与位置

当前最新版本 22（`src/store/migrations.rs:46-108`）。新增 `(23, "add_host_dimension", m_023_add_host_dimension)`。

`MIGRATIONS` 是不可变历史，只追加，不修改既有条目。

## 迁移内部顺序

顺序不可调换，否则键重写会作用在错误的行集上：

1. **不在 `m_023` 内备份。** `MigrationFn = fn(&Transaction<'_>)`（`store/migrations.rs:13-14`），运行器在 `BEGIN IMMEDIATE` 之后才调用迁移（`store/migrations.rs:209-214`）。磁盘升级备份放在 `Store::bootstrap`：读到 `schema_version == 22` 且 `db_path` 是文件时，用独立连接 `PRAGMA wal_checkpoint(TRUNCATE)`，再复制到 `backups/llmusage.db.pre-0.23-host`（已存在则不覆盖，与 `store/schema.rs:310-322` 同形），然后才跑迁移。备份失败则中止，不进入 v23。`run_migrations_for_test` 与内存库不走该分支。
2. `CREATE TABLE host`，插入 `local` 行。
3. `ensure_column` 追加 `host_id TEXT NOT NULL DEFAULT 'local'` 到七张表（含随后要建表复制的四张，使 DEFAULT 落在复制源上）。
4. 重写六个键列（父 design.md §3.2 的 SQL）。
5. 四张主键变更表建表复制（`usage_bucket_30m`、`source_file`、`source_cursor`、`source_sync_status`），沿用 `m_014` 的 `__v14` 命名与复制方式（`store/migrations.rs:672-710`）。
6. 建新索引 `usage_event(host_id, source, event_at)`、`source_file(host_id, source, state)`。

步骤 4 在步骤 5 之前：主键变更表都不含 event_key，两步无耦合；但把键重写放在建表复制之前可以避免复制后再更新更大的中间表。

## SyncShard 与前缀施加

`SyncShard` 增加 `pub host_id: String` 与 `pub host_prefix_applied: bool`。`SyncShard::new(source)` 保持现有签名并填 `"local".to_string()`、`host_prefix_applied: false`，新增 `SyncShard::new_for_host(source, host_id)`。这样九个 parser 无需改动。

前缀在 `commit_shard_inner` 的入口处一次性施加，改写 shard 内的键，然后走既有写入流程。改写规则：

| 字段                          | 改写                                                 |
| ----------------------------- | ---------------------------------------------------- |
| `UsageEvent.event_key`        | `{host}:{key}`                                       |
| `UsageTurn.turn_key`          | `turn:{host}:{key去掉 turn: 前缀}`                   |
| `UsageToolCall.event_key`     | `{host}:{key}`                                       |
| `UsageToolCall.turn_key`      | `turn:{host}:{key去掉 turn: 前缀}`                   |
| `UsageToolCall.tool_call_key` | `tool:{source}:{host}:{key去掉 tool:{source}: 前缀}` |
| `RawRecord.event_key`         | `{host}:{key}`                                       |

幂等要求：若 `host_prefix_applied` 已为 true 则跳过改写；否则改写一次并置 true。不要用 `starts_with("{host}:")`：`turn_key` / `tool_call_key` 不以 `{host}:` 开头；`host_id` 与 source 名冲突由注册阶段拒绝（父 R1.6）。

## SourceFileStore 签名变更

`store/source_file.rs` 的以下函数追加 `host_id: &str`：`counts`、`tracked_paths`、`sweep_missing`、`mark_inventory_seen`、`lossy_rebuild_risk`、`upsert_live_in_tx`、`update_missing_with_conn`、`delete_for_source_in_tx`。`lossy_rebuild_risks` 返回值增加 host 字段。

本子任务所有调用方一律传 `"local"`，包括：`parsers/driver.rs`、`store/sync_writer.rs`、`commands/diagnostics.rs`、`commands/sync.rs`、`commands/serve.rs` 的 `lossy_rebuild_risk`、以及各 parser 的 `mark_inventory_seen` / `tracked_paths`（`codex` / `claude` / `kimi_code` / `pi` / `grok` / `dsh` / `antigravity`）。按 host 限定扫描的行为改动属于 C4。

主键变更后必须改写的冲突子句（漏改则本地 sync 在 SQL 层失败）：

| 文件 | 现有冲突目标 |
| --- | --- |
| `store/sync_writer.rs` `write_cursor_batch_tx` | `ON CONFLICT(source, cursor_key)` |
| `store/sync_writer.rs` bucket upsert | `ON CONFLICT(source, provider_label, model, hour_start, project_hash)` |
| `store/cursor.rs` `save_opencode_cursor` / `save_zcode_cursor` | `ON CONFLICT(source, cursor_key)` |
| `store/sync_status.rs` upsert / `mark_recent_completed` | `ON CONFLICT(source)` |
| `store/source_file.rs` `upsert_live_in_tx` | `ON CONFLICT(source, file_path)` |
| `store/source_file.rs` `Store::mark_source_file_deleted` | `ON CONFLICT(source, file_path)` |

`integration_install` 的 `ON CONFLICT(source)` 是另一张表，不改。

## 兼容性

- 不可逆。降级到旧二进制命中 `SchemaTooNew`（`store/schema.rs:41-46`）。
- `require_initialized` 与 `latest_schema_version` 无需改动，版本号自动前进。
- 公开 API 变更：`Store::reset_for_source` 增加参数，`SourceFileStore` 多个方法增加参数，`SyncShard` 增加字段。`tests/public_api.rs` 需同步更新。
