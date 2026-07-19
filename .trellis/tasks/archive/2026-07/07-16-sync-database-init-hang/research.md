# Sync Database Initialization Hang Research

## Conclusion

The installed `llmusage 0.9.2` is not blocked creating the database or running
a schema migration. Bootstrap sees the live database's active catalog
`static-v1`, detects that the embedded catalog is `static-v2`, and automatically
calls the full-database pricing recomputation. The final orphan-bucket cleanup
uses a correlated `NOT EXISTS` query without a complete event bucket-key index.
On this database that query is the high-confidence blocking operation.

## Live Evidence

- Installed binary: `C:\Users\lyh\.cargo\bin\llmusage.exe`, version `0.9.2`.
- Database: `C:\Users\lyh\.llmusage\llmusage.db`, 1,159,815,168 bytes,
  WAL mode, schema version 14.
- Row counts: 539,146 `usage_event` rows and 7,153 `usage_bucket_30m` rows.
- `meta('pricing_catalog_version')` is still `static-v1`.
- 534,587 priceable events already have `pricing_source='static-v2'`; the
  remaining 4,559 are intentionally unpriced. Bucket rows still retain the old
  `static-v1`/unpriced/mixed rollups. This proves pass 1 committed while the
  closing bucket/meta transaction did not commit.
- Every persisted bucket has a matching event bucket key; there are zero actual
  orphan buckets.
- The exact read-only orphan predicate did not finish within five seconds and
  executed at least 35,980,000 SQLite VM steps before interruption.
- Its query plan scans every bucket and runs a correlated lookup that can only
  constrain `usage_event` by `source`, because no index covers
  `(source, provider_label, model, hour_start, project_hash)`.
- A one-pass set comparison found zero orphans in 1.211 seconds; the equivalent
  `EXCEPT` query completed in 0.936 seconds.
- No `llmusage.exe` process remained by the time diagnostics began, so direct
  stack/CPU sampling of the original process was unavailable.

## Code Path

1. `src/commands/sync.rs:70` renders `BootstrapStarted` as
   `初始化数据库...` before calling store bootstrap.
2. `src/store/schema.rs:59` reads schema version and runs migrations; this live
   database is already at the latest version 14.
3. `src/store/schema.rs:68` then calls `upgrade_embedded_pricing_if_needed`
   without a separate progress event.
4. `src/store/pricing_catalog.rs:398` triggers recomputation when the active
   catalog is an older `static-*` version.
5. `src/store/mod.rs:342` commits event updates in pages of 5,000.
6. `src/store/mod.rs:434` starts the final bucket/meta transaction. The
   correlated orphan deletion at `src/store/mod.rs:452` must finish before the
   catalog version is advanced and the transaction commits at line 496.

## Why It Repeats

Event pages commit independently, but the catalog version changes only in the
final transaction. Interrupting the slow orphan cleanup leaves the event rows
updated while `pricing_catalog_version` remains `static-v1`. The next bootstrap
therefore starts the entire recomputation again.

## Test Gap

- `recompute_costs_deletes_orphan_buckets` uses one live bucket and one orphan.
- `paged_recompute_can_retry_after_a_later_page_fails` uses 5,001 events in one
  bucket.
- Both focused tests pass, but neither exercises many buckets against a large
  same-source event table, so they do not expose the bucket-count times
  source-event-count query shape.

## Repair Direction

- Reuse the already-built in-memory `buckets` key map: read existing bucket
  primary keys once and delete only keys absent from the map. This avoids the
  correlated event scan and requires no permanent schema index.
- Add a focused many-bucket regression that structurally prevents a correlated
  full event scan; keep timing assertions secondary to avoid flaky CI.
- Add explicit pricing-upgrade progress events so catalog recomputation is not
  presented as generic database initialization.
- Do not manually advance `pricing_catalog_version` on the live database: the
  bucket rollups have not committed under `static-v2` yet.
