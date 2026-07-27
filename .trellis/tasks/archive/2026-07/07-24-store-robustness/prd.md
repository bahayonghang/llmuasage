# Store 健壮性：schema 降级防护/备份一致性/reset 事务（DATA-004/005、REL-006）

## Goal

修复 Store 层三个健壮性缺口：schema 版本降级/损坏防护、WAL 模式下的一致性备份、destructive reset 的原子性。

## 覆盖发现（已核实）

- **DATA-004（P2）**：`src/store/migrations.rs:99-104` malformed schema version `parse().ok().unwrap_or(0)` 被当 0（会在既有库上重跑全部 migration）；`134-136` 行 `if *version <= current { continue; }`——`current > latest` 时全部跳过、无 fail-fast，旧 binary 可在新 schema 上继续 bootstrap/写入，造成静默破坏。
- **DATA-005（P2）**：`src/store/connection.rs:40` 开启 WAL；`src/store/schema.rs:243-250` pre-0.5 backup 仅 `fs::copy` 主 DB 文件，不含 WAL、不用 SQLite backup API。有未 checkpoint page 或并发写时备份可能陈旧/不一致。
- **REL-006（P3）**：`src/store/schema.rs:223-241` public `reset_usage_data()` 以多条 DELETE `execute_batch` 执行、无显式事务；中途错误留下部分 reset。当前未发现 production caller，但作为 public Store API 是不安全默认。

## Requirements

1. malformed schema version 是 hard error（不再 unwrap_or(0)）；新增 `SchemaTooNew { db, binary }` 错误：`current > latest` 时 fail-fast，可选 read-only compatibility 模式。
2. 备份改用 SQLite online backup API 或 `VACUUM INTO`（在写协调下执行）；备份前 checkpoint、备份后 `integrity_check`。
3. `reset_usage_data` 包裹 immediate transaction，或降为 `pub(crate)` 并只暴露安全 orchestration。

## Acceptance Criteria

- [ ] 单测覆盖 malformed version 与 future version：均明确拒绝，不静默继续（在旧实现上稳定失败）。
- [ ] 含未 checkpoint WAL 数据的备份可完整恢复（动态实验测试）。
- [ ] reset 中途注入错误后，数据要么全清要么全留，无部分 reset。
- [ ] 正常升级路径（v0→latest 及中间版本）不回归。

## Notes

- DATA-004 是审计"当天阻断项"之一，可先行落地。
- 备份部分建议在 07-24-write-fencing-coordinator 落地后接入其写协调；若先行，需注明串行前提。
