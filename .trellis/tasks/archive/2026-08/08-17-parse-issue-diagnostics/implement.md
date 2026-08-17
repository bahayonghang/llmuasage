# Implement: parse issue 诊断与 ZCode skip 去重

## Order

1. Domain：`ParseIssueSample.reason`、`record` 签名、sanitize、serde 单测。
2. Schema v22：`source_cursor.last_skipped_at` / `last_skipped_ids_json`；`ZcodeCursor` 读写。
3. ZCode：按行计数、reason、推进/重置 skip 水位；`--recent-days` 与取消不写水位。
4. CLI：sync 摘要与 source-status 打印 reason；有 reason 时不打印 `@0`。
5. Driver：非零 parse issue 时一条 info 事件。
6. 契约：`source-sync-contracts.md`、`runtime-log-contracts.md`。
7. 测试：domain / zcode 单测、sync_summary、source-status、`sync_regression` 去重与重建。

## Validation

```
cargo test domain::models parsers::zcode commands::sync_summary commands::source_status -- --test-threads=1
cargo test --test sync_regression zcode_ -- --test-threads=1
python scripts/ci-rust.py
```

聚焦回归：`zcode_skips_error_and_cancelled_rows_and_counts_them` 必须改成「第一次=2、第二次=0」，并补「新未完成行只报一次」与「重建重置 skip 水位」。

## Risky files

- `src/domain/models.rs` — `record` 签名会碰到所有 `issues.record` 调用点
- `src/parsers/zcode.rs` — 计数查询与水位
- `src/store/cursor.rs` / `src/store/migrations.rs` — 新列
- `src/commands/sync_summary.rs` / `src/commands/source_status.rs`
- `src/parsers/driver.rs`
- `.trellis/spec/llmusage/backend/source-sync-contracts.md`
- `.trellis/spec/llmusage/backend/runtime-log-contracts.md`

## Rollback

回退分支。v22 列可留。不要在 rollback 后手改用户 `parse_issues_json`。

## Before task.py start

- `prd.md` 已收敛，无未决产品问题
- `design.md` / `implement.md` 已写
- `implement.jsonl` / `check.jsonl` 已换成真实 spec 条目
- 用户已明确批准本规划摘要
