# Design - Sync Bootstrap Pricing Performance And Progress

## 1. Decision Summary

The implementation will make the final pricing bucket reconciliation linear by
reusing the bucket-key map already produced during event repricing. It will add
an internal bootstrap progress envelope, map pricing upgrade phases into the
existing `SyncEvent` stream, and add structured phase logs without changing the
default log filter or SQLite schema.

## 2. Current And Target Flow

Current:

```text
BootstrapStarted
  -> schema migrations
  -> static-v1 -> static-v2 event pages
  -> for every bucket: correlated usage_event lookup
  -> bucket/meta commit
  -> LockWaiting
```

Target:

```text
BootstrapStarted
  -> migration events (unchanged)
  -> PricingUpgradeStarted(total events)
  -> event pages + throttled PricingUpgradeProgress
  -> PricingBucketReconcileStarted(bucket count)
  -> one bucket-key read + PK updates/deletes
  -> bucket/meta commit
  -> PricingUpgradeFinished
  -> LockWaiting
```

## 3. Linear Reconciliation

`recompute_costs_with_meta` keeps the existing paged event updates and
`HashMap<BucketKey, PricingRollup>`. In the final transaction it will:

1. Query only bucket primary-key columns from `usage_bucket_30m` into a bounded
   `Vec<BucketKey>`.
2. Iterate `&buckets` to update pricing columns by the existing composite
   primary key.
3. Iterate persisted keys and execute a prepared primary-key delete only when
   `!buckets.contains_key(key)`.
4. Apply activation meta changes and commit as today.

The final phase does not reference `usage_event`. Complexity becomes O(E) for
the existing event pass plus O(B) reconciliation, instead of O(B * E_source).
The map already exists, so additional memory is only the persisted bucket-key
vector.

The internal result will carry `updated_events`, `bucket_count`,
`deleted_orphan_buckets`, and phase elapsed times. Existing public methods may
continue returning their current `usize` event count.

## 4. Bootstrap Progress Types

Add crate-internal Store types, re-exported only as needed inside the crate:

```text
BootstrapProgressEvent
  Migration(MigrationProgressEvent)
  PricingUpgradeStarted {
    from_version, to_version, total_events
  }
  PricingUpgradeProgress {
    from_version, to_version, processed_events, total_events, elapsed_ms
  }
  PricingBucketReconcileStarted {
    to_version, bucket_count
  }
  PricingUpgradeFinished {
    from_version, to_version, updated_events, bucket_count,
    deleted_orphan_buckets, elapsed_ms
  }
```

`Store::bootstrap_with_migration_events` remains source-compatible. A new
crate-internal bootstrap-with-progress entrypoint adapts migration callbacks
into the unified envelope and passes the same sink into the embedded pricing
upgrade. Other Store callers keep using `bootstrap()` and receive structured
logs but no UI callback.

`recompute_costs_with_meta` gains an optional internal progress sink. Public
catalog/doctor APIs keep their signatures and pass no UI sink.

## 5. Sync Event Mapping

Add corresponding additive `SyncEvent` variants using the existing
`#[serde(rename_all = "snake_case", tag = "event")]` contract. One conversion
helper maps `BootstrapProgressEvent` into `SyncEvent`; both human and
`--json-events` paths use it.

Human copy:

| Event | stderr text shape |
| --- | --- |
| BootstrapStarted | `检查数据库 schema 与定价目录...` |
| PricingUpgradeStarted | `升级定价目录 static-v1 -> static-v2：共 539146 条事件...` |
| PricingUpgradeProgress | `升级定价目录 static-v1 -> static-v2：已处理 250000/539146 条（46.4%）` |
| PricingBucketReconcileStarted | `事件定价完成，正在对账 7153 个汇总桶...` |
| PricingUpgradeFinished | `定价目录 static-v2 升级完成：539146 条事件，7153 个桶（...ms）` |

TTY output replaces the active line and writes a newline on reconcile/finish
phase boundaries. Non-TTY output receives only the throttled lifecycle events.
No pricing events are emitted for a no-op/pinned bootstrap.

The new NDJSON variants are additive. Existing event names and stdout purity
remain unchanged; consumers that ignore unknown event kinds continue working.

## 6. Structured Logging

Use stable fields instead of interpolating operational data only into messages:

- `operation = "pricing_recompute"`
- `phase = "started" | "events" | "bucket_reconcile" | "finished"`
- `from_version`, `to_version`
- `processed_events`, `total_events`, `bucket_count`
- `deleted_orphan_buckets`, `elapsed_ms`

Levels:

- `info`: start, reconcile start, finish.
- `debug`: throttled page progress.
- `warn`: once when elapsed time first crosses 30 seconds; after reconciliation
  if that phase itself exceeds its expected budget.
- `error`: phase failure before returning the original error.

No default filter or rotation changes. Human progress remains visible even when
`RUST_LOG` and `LLMUSAGE_LOG` retain their default warn level. Logs contain no
event keys, local paths, prompts, or raw catalog contents.

## 7. Progress Throttling

Count total events once before page processing. After each committed 5,000-row
page, emit progress when any condition holds:

- at least one second since the prior event;
- at least 25,000 additional rows were processed;
- processed rows equal total rows.

The slow-operation warning is independent and emitted once on the first page
boundary after 30 seconds. This bounds non-TTY/NDJSON volume while ensuring a
large active upgrade changes state regularly.

## 8. Failure And Compatibility Semantics

- Event pages remain independently committed; no new long transaction is
  introduced.
- Bucket changes and activation metadata remain in one final transaction.
- A failure emits no `PricingUpgradeFinished` and leaves active metadata on the
  prior catalog, preserving retry behavior.
- Progress callbacks are observational only and do not write durable progress
  state.
- No schema version bump, new dependency, catalog-rate change, or public Store
  API break is required.
- `SyncEvent` is an additive serialized API change; all exhaustive internal
  matches must handle or deliberately ignore the new variants.

## 9. Validation Design

Store tests:

- Many recomputed/persisted buckets plus an orphan: updates match, only the
  orphan is deleted, and reconciliation executes no SQL against `usage_event`.
- Zero-event/zero-bucket and no-orphan cases.
- Progress count is monotonic and ends at total; phase ordering is stable.
- Failure before final commit leaves activation metadata unchanged; retry
  finishes consistently.

CLI/integration tests:

- Human line formatting includes versions, counts, percentage, phase, and time.
- A static-v1 isolated fixture emits ordered pricing NDJSON events and only
  valid JSON on stdout.
- A current/pinned catalog emits no pricing-upgrade lifecycle.
- `LLMUSAGE_LOG=info` subprocess output contains structured start/reconcile/end
  entries; an injectable clock or focused unit isolates the one-time 30-second
  warning behavior without sleeping.

Performance validation:

- Keep CI assertions structural and deterministic rather than relying only on
  wall-clock thresholds.
- Use SQLite backup into an isolated runtime snapshot of the diagnosed live DB.
- Measure event phase and bucket-reconcile phase separately from emitted logs.
- Require bucket reconciliation under two seconds on this machine, successful
  static-v2 activation, matching bucket/event keys, and visible progress.

## 10. Documentation And Contracts

Update:

- `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`
- `.trellis/spec/llmusage/backend/source-sync-contracts.md`
- `README.md` and `README.zh-CN.md`
- `docs/reference/cli.md` and `docs/zh/reference/cli.md`
- first-sync guides when they show lifecycle behavior

Document additive pricing lifecycle events, progress fields, no-op behavior,
and the fact that a first run after an embedded catalog upgrade may reprice
historical events before source scanning begins.

## 11. Rejected Alternatives

- Permanent event bucket-key index: unnecessary ongoing storage/write cost and
  schema migration for a maintenance phase whose key set already exists.
- `EXCEPT` cleanup: linear enough but redundantly scans all events again.
- Meta advance before bucket commit: violates the activation contract.
- Logging only: improves diagnosis but leaves the performance bug and default
  terminal ambiguity unresolved.
- Progress only: makes the wait understandable but preserves pathological SQL.
- Persistent resume checkpoints: useful future work, but materially expands
  metadata and recovery semantics beyond this bug.

## 12. Rollback

Because there is no migration, rollback is code-only. If progress plumbing
regresses integrations, the linear reconciliation and structured logs can be
retained while the additive SyncEvent variants are reverted. Never roll back by
editing live catalog metadata or deleting SQLite WAL files.
