# Optimize Sync Bootstrap Pricing Recompute And Progress

## Goal

Make the one-time embedded pricing-catalog upgrade complete predictably on
large databases and make every long-running bootstrap phase visibly progress,
so `llmusage sync` is not mistaken for a dead process while displaying only
`初始化数据库...`.

## Background

- The installed `llmusage 0.9.2` upgrades an unpinned `static-v1` catalog to
  embedded `static-v2` during Store bootstrap.
- The live database is 1,159,815,168 bytes with 539,146 events and 7,153
  buckets. Its schema is already v14, so schema migration is not the stall.
- Event repricing completed, but the final bucket/meta transaction did not:
  534,587 priceable events use `static-v2`, bucket rollups remain on the old
  catalog, and `pricing_catalog_version` remains `static-v1`.
- The final orphan cleanup at `src/store/mod.rs:452` executes a correlated
  `NOT EXISTS` lookup from every bucket into `usage_event`. On the live database
  its read-only predicate exceeded five seconds and 35,980,000 SQLite VM steps,
  while a one-pass key-set comparison completed in about one second.
- `src/commands/sync.rs:632` renders the whole bootstrap as
  `初始化数据库...`; pricing recompute has no human or NDJSON progress events.
- The existing `.trellis/config.yaml` modification is user-owned and must be
  preserved.

## Requirements

### R1 - Linear Bucket Reconciliation

- Replace the correlated orphan-bucket cleanup with an algorithm that reuses
  the in-memory recomputed bucket-key map.
- Read persisted bucket primary keys once, update recomputed bucket pricing,
  and delete only persisted keys absent from the recomputed key map.
- The final reconciliation phase must not query `usage_event`.
- Preserve the current final-transaction boundary: bucket updates, orphan
  deletion, activation metadata changes, and commit remain atomic.
- Do not add a permanent SQLite index or schema migration for this fix.

### R2 - Bootstrap Pricing Progress Contract

- Keep the existing migration callback API compatible and add an internal
  bootstrap progress path used by `sync`.
- Emit ordered pricing-upgrade lifecycle events:
  `started`, throttled `progress`, `bucket_reconcile_started`, and `finished`.
- Started/progress events include source and target catalog versions plus
  processed and total event counts. Reconcile/finished events include bucket
  counts, deleted orphan count, and elapsed milliseconds where applicable.
- Emit progress after committed pages, throttled to at most once per second or
  every 25,000 events, and always emit the final page.
- Emit no pricing-upgrade events when the embedded catalog is already current
  or a user snapshot/overlay is pinned.

### R3 - Human And NDJSON Output

- Replace the ambiguous bootstrap text with
  `检查数据库 schema 与定价目录...`.
- Human stderr must show catalog versions, processed/total events, the bucket
  reconciliation phase, and final elapsed time.
- TTY progress may update one line; non-TTY output must remain bounded by the
  throttled event stream.
- `sync --json-events` must serialize the same new events as additive
  snake_case NDJSON variants and continue writing only valid NDJSON to stdout.
- Existing migration, lock, source, finish, failure, and cancellation events
  retain their current meaning and order.

### R4 - Structured Runtime Logs

- Log pricing recompute start, bucket reconciliation, and completion at `info`
  with stable fields: operation, phase, catalog versions, counts, and elapsed
  milliseconds.
- Log throttled page progress at `debug`.
- When repricing exceeds 30 seconds, emit one `warn` progress record so the
  default warn-level file log proves the process is still advancing.
- Log failures at `error` with the failed phase and processed counts before
  propagating the original error.
- Do not log event keys, model prompts, local source paths, or catalog contents.
- Keep the current default logging level and file rotation behavior unchanged.

### R5 - Regression Coverage And Documentation

- Add a many-bucket reconciliation regression including at least one orphan and
  prove the reconciliation SQL does not read `usage_event`.
- Add bootstrap progress tests for ordering, monotonic counts, successful meta
  activation, no-op bootstrap, and interrupted retry behavior.
- Extend human-output and `sync --json-events` subprocess tests for the new
  lifecycle events and stdout purity.
- Update pricing/source-sync contracts and matching English/Chinese CLI docs.

## Acceptance Criteria

- [ ] AC1: Bucket reconciliation is O(bucket_count) after the existing event
      pass and contains no correlated or full scan of `usage_event`.
- [ ] AC2: On an isolated snapshot of the diagnosed database, the final bucket
      reconciliation completes in under two seconds on this machine, advances
      `pricing_catalog_version` to `static-v2`, and leaves zero orphan buckets.
- [ ] AC3: A long embedded upgrade visibly reports catalog versions,
      processed/total events, bucket reconciliation, and completion instead of
      remaining on a generic initialization line.
- [ ] AC4: `--json-events` remains parseable NDJSON-only stdout and exposes the
      same ordered pricing lifecycle without breaking existing event variants.
- [ ] AC5: Structured logs contain start/reconcile/finish fields; a run lasting
      over 30 seconds records exactly one slow-progress warning.
- [ ] AC6: Failure before the final commit leaves activation metadata unchanged;
      retry produces consistent event and bucket pricing.
- [ ] AC7: Focused tests, serial full Rust tests, strict Clippy, docs build, and
      diff checks pass.
- [ ] AC8: The live database is not used for implementation tests or modified
      before the isolated-snapshot validation passes.

## Out Of Scope

- Persistent page-level resume checkpoints across process restarts.
- Replacing the existing 5,000-row event repricing pages with a new bulk
  pricing engine.
- Changing pricing rates, matcher behavior, schema version, or catalog
  activation semantics.
- Redesigning parser progress, the in-memory job registry, or unrelated command
  bootstrap output.
- Manually advancing catalog metadata or rebuilding the user's live database.
