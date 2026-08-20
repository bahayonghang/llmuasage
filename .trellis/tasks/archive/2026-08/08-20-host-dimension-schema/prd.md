# C1：host 维度 schema 与 event_key 前缀

父任务：`.trellis/tasks/08-20-ssh-remote-host-import`

## Goal

在 SQLite schema 中建立 host 维度，并把 `event_key` 及其派生键统一改为带 host 前缀，使多主机数据可以共存且互不覆盖。本子任务完成后，本地数据行为不变，但 schema 已经能容纳远端主机。

## Scope

覆盖父任务 R2（全部）与 R1.1、R1.2（`host` 表结构与 `local` 行）。不含 SSH 传输、CLI `remote` 子命令、读取层 host 过滤。

前置：无。后继 C2、C3 都依赖本子任务。

## Requirements

- R2.1 `usage_event`、`usage_turn`、`usage_tool_call`、`usage_bucket_30m`、`source_file`、`source_cursor`、`source_sync_status` 增加 `host_id`。
- R2.2 主键调整为 `usage_bucket_30m`(host_id, source, provider_label, model, hour_start, project_hash)、`source_file`(host_id, source, file_path)、`source_cursor`(host_id, source, cursor_key)、`source_sync_status`(host_id, source)。
- R2.3 存量行回填 `host_id = 'local'`。
- R2.4 `event_key` 统一为 `{host_id}:{原键}`，含本地主机。
- R2.5 迁移同步重写六个键列（父 design.md §3.2）。
- R2.6 `Store::reset_for_source` 追加 host 参数，raw 删除改为子查询并提到 `usage_event` 删除之前。
- R2.7 前缀由 `commit_shard` 集中施加；`SyncShard` 增加 `host_id` 字段，默认 `local`；parser 输出保持不含 host。
- R1.1 建 `host` 表（父 design.md §2.1）。
- R1.2 迁移写入 `host_id='local'` 行。
- 磁盘库从 v22 升到 v23 时，`Store::bootstrap` 在打开 v23 事务之前备份到 `backups/llmusage.db.pre-0.23-host`。内存库 / `run_migrations_for_test` 跳过备份。
- `SourceFileStore` 的公开与内部方法追加 host 参数（父 design.md §5.1 列出的八个函数）。
- 主键变更后同步改写所有 `ON CONFLICT` 写入点：`sync_writer.rs` 的 cursor 与 bucket、`cursor.rs` 的 OpenCode/Zcode cursor、`sync_status.rs`、`source_file.rs` 的 `upsert_live_in_tx` 与 `Store::mark_source_file_deleted`。

## Acceptance Criteria

- [ ] AC1 从 v22 库升级后：`usage_event` 行数不变；按 source 分组的 `total_tokens` 与 `cost_with_cache_usd` 合计不变；`usage_turn` 经 `substr(turn_key, 6)` 能关联到 `usage_event` 的行数不变；`usage_tool_call` 经 `event_key` 能关联到 `usage_event` 的行数不变。
- [ ] AC2 迁移后对同一份未变更的本地产物再次 `sync`，`events_inserted` 为 0。
- [ ] AC3 `reset_for_source(codex, "local")` 只删除 codex 的 `usage_event_raw` 行；其他 source 的 raw 行全部保留。
- [ ] AC1b 全新建库路径产出的 schema 与升级路径一致（沿用 `run_migrations_for_test(&MIGRATIONS[..N])` 对比模式，`store/migrations.rs:1626-1796`）。
- [ ] AC1c `usage_tool_call.turn_key` 为 NULL 的行在迁移后仍为 NULL。
- [ ] AC1d 磁盘库从 v22 升级时，`bootstrap` 在 v23 事务开始前生成 `backups/llmusage.db.pre-0.23-host`；用 rusqlite 打开该备份、不跑 v23，读到的 `schema_version` 为 22。内存库升级路径不要求该文件，且不得因备份缺失而失败。
- [ ] AC6a 同一 `file_path` 在两个不同 `host_id` 下可同时存在于 `source_file` 与 `source_cursor`，互不覆盖。
- [ ] `cargo test --all-features -- --test-threads=1` 通过。

## Out of Scope

- SSH 传输与远端命令。
- `remote add/list/remove` 子命令。
- 读取层 `--host` 过滤与 dashboard。
- 远端生命周期语义（`missing` 扫描按 host 限定的调用侧改动留给 C4；本子任务只改函数签名并让本地调用方传 `local`）。
