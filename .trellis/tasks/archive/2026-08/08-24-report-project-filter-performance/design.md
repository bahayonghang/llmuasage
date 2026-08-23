# Design

## Period Aggregate Core

引入内部 `PeriodSpec`（Daily/Weekly/Monthly）和 `PeriodAggregateBundle`：

```text
filtered bucket rows
  -> group by period + source + host + model + pricing
  -> bundle.overall
  -> bundle.by_source
  -> bundle.by_host
  -> existing Daily/Weekly/Monthly DTO projectors
```

`PeriodSpec` 只提供 period key/DTO 映射，不携带 SQL 或 branching policy。现有 public loaders delegate 到 bundle/projectors；`load_unified_report` 直接消费一次 bundle。

## Project Resolution

1. Normalize needle exactly like current matcher: trim + lowercase; empty matches all current project rows.
2. Query candidate `(hash,label,ref)` from `project_dim UNION current filtered distinct bucket projects`。
3. 在一个 canonical matcher 中产生 deduped hashes。
4. totals bucket query 添加 exact hash predicate；若无 hash，立即返回 empty bundle。

Stale `project_dim` 只会增加 candidate hash，后续 current filter 没有 bucket/event 时不会产生输出。历史/remote bucket 没有 dimension 时仍可通过 union 被发现。

## Exact Conversation Counts

对 project-filtered daily/source/host rows运行窄 SQL：从 `usage_event` 只计算 period key、source、host 和 canonical session identity 的 distinct count。canonical identity 保留现有 `session_id -> source_path_hash -> event_key fallback` 规则。聚合在 SQLite 内完成，不构造 `RawEventRow/EventRow`。

若 EQP 仍为不可接受的全扫描，先实验 project/time covering index；只有读收益、迁移时间、DB 增量和 sync write amplification 全部过门才保留。

## Equivalence Oracle

实施期间保留旧 event matcher 为 test-only oracle：对相同 fixture 比较 period rows、totals、model breakdowns、conversation counts、notes 和 ordering。生产切换后删除旧生产分支，test oracle 可保留在 fixture helper 中。

## Performance Harness

固定 SQLite fixture 先 seed/ANALYZE，再单独计时查询；一次 warm-up 后五次样本，nearest-rank p95。trace 统计 statement，EQP 保存 detail/opcode，输出不包含 project 值或 rows 内容。

## Rollback

顺序：generic projectors → one-pass bundle → project resolver → bucket totals → narrow conversations → optional index。任何语义差异先回退 fast path，保留已验证的结构复用；index 最后落且可单独移除。

