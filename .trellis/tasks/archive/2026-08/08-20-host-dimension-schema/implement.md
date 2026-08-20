# C1 执行清单

设计依据：父 `design.md` §2.1、§3、§5.1、§7；本任务 `design.md`。

## 顺序清单

- [ ] 1. `src/store/schema.rs`：磁盘库 `schema_version == 22` 时，在跑 v23 之前按 pre-0.5.0 同形备份到 `backups/llmusage.db.pre-0.23-host`。内存库跳过。备份失败则中止磁盘升级。
- [ ] 2. `src/store/migrations.rs`：追加 `(23, "add_host_dimension", m_023_add_host_dimension)`。函数内只做建表、ensure_column、键重写、主键复制、索引；**不**做文件备份。
- [ ] 3. 迁移测试：从 `MIGRATIONS[..22]` 升级到 `[..23]`，与全新建库对比 schema；覆盖键重写前后的行数、总量、cost 合计、父子关联数、NULL `turn_key` 保持。磁盘夹具覆盖 AC1d；内存路径不得因备份缺失失败。沿用 `store/migrations.rs:1626-1796` 的既有测试形状。
- [ ] 4. `src/store/mod.rs`：`SyncShard` 增加 `host_id` 与 `host_prefix_applied`，新增 `SyncShard::new_for_host`；`RawRecord` 保持结构不变。
- [ ] 5. `src/store/sync_writer.rs`：在 `commit_shard_inner` 入口按 `host_prefix_applied` 施加 host 前缀（本任务 design.md 的改写表）；`write_cursor_batch_tx`、`write_source_file_seen_tx`、bucket upsert 的 `ON CONFLICT` 纳入 `host_id`。
- [ ] 6. `src/store/schema.rs`：`reset_for_source` 增加 host 参数；raw 删除改为子查询并移到 `usage_event` 删除之前；其余删除语句追加 `host_id` 条件。
- [ ] 7. `src/store/source_file.rs`：八个函数追加 `host_id` 参数；`LossyRebuildRisk` 增加 host 字段；`upsert_live_in_tx` 与 `Store::mark_source_file_deleted` 的 `ON CONFLICT` 纳入 `host_id`。
- [ ] 8. `src/store/cursor.rs`：`save_opencode_cursor` / `save_zcode_cursor` 的 `ON CONFLICT` 与写入列纳入 `host_id`（本子任务传 `"local"`）。
- [ ] 9. `src/store/sync_status.rs`：upsert 与 `mark_recent_completed` 的 `ON CONFLICT(source)` 改为含 `host_id`。
- [ ] 10. 更新调用方一律传 `"local"`：`src/parsers/driver.rs`、`src/commands/sync.rs`、`src/commands/diagnostics.rs`、`src/commands/serve.rs`、`src/store/sync_writer.rs`、以及 `src/parsers/{codex,claude,kimi_code,pi,grok,dsh,antigravity}.rs` 的 `mark_inventory_seen` / `tracked_paths`。
- [ ] 11. 新增 `host` 表的读写 API：`Store::hosts()` 子 store，提供 `list`、`get_by_label`、`upsert`、`remove`、`set_watermark`、`record_contact`。本子任务只需 `list` 与 `get_by_label` 有调用方，其余为 C2 预留但同批实现以避免二次改 store。
- [ ] 12. `tests/public_api.rs` 与 `tests/m2_raw_archive_logs.rs` 的 `reset_for_source` 调用同步签名。
- [ ] 13. `cargo fmt`，然后跑验证命令。

## 验证命令

```bash
cargo test --test sync_regression -- --test-threads=1
cargo test --test source_file_state -- --test-threads=1
cargo test --test token_accounting_parity -- --test-threads=1
cargo test --test public_api -- --test-threads=1
cargo test --all-features -- --test-threads=1
cargo clippy --all-targets --all-features -- -D warnings
```

## 审查门

进入 C2 之前必须确认 AC1、AC1b、AC1c、AC1d、AC2、AC3、AC6a 全部通过。迁移不可逆，这是唯一的拦截点。

## 风险与回滚

| 风险                                    | 位置                   | 处置                           |
| --------------------------------------- | ---------------------- | ------------------------------ |
| 键重写漏掉某一列，父子关联断裂          | `migrations.rs` 步骤 4 | AC1 断言四类关联行数           |
| `substr` 对 NULL `turn_key` 写入非 NULL | `usage_tool_call` 重写 | AC1c 专项断言                  |
| 建表复制丢列或丢索引                    | 步骤 5                 | 与全新建库 schema 对比（AC1b） |
| 前缀重复施加                            | `commit_shard_inner`   | `host_prefix_applied` 标志，禁止 `starts_with("{host}:")` |
| 主键变更后旧 `ON CONFLICT` 失效         | `cursor.rs` / `sync_status.rs` / bucket upsert / `mark_source_file_deleted` | 步骤 5–9 列入清单；G1 全量测试拦住 |
| 磁盘备份失败后继续迁移                  | `schema.rs` bootstrap  | 备份失败必须返回错误，不进入 v23 |
| 内存测试因备份失败中止                  | `run_migrations_for_test` | 内存路径跳过备份 |

回滚点：`backups/llmusage.db.pre-0.23-host`。迁移本身不提供降级路径。
