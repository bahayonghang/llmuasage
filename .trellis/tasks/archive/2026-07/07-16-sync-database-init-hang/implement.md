# Implementation Plan - Sync Bootstrap Pricing Performance And Progress

## Gate 0 - Approval And Baseline

- [ ] User reviews and approves `prd.md`, `design.md`, and this plan.
- [ ] After approval, run
      `python ./.trellis/scripts/task.py start 07-16-sync-database-init-hang`.
- [ ] Load `trellis-before-dev` and the curated implement context before editing
      product code.
- [ ] Recheck `git status`; preserve the existing `.trellis/config.yaml` change
      and any other unrelated work.
- [ ] Record the focused test baseline and do not touch the live database.

## Step 1 - Linear Bucket Reconciliation

- [ ] Extract the final bucket reconciliation into a focused Store helper that
      receives the recomputed `HashMap<BucketKey, PricingRollup>` and active
      transaction.
- [ ] Read persisted bucket primary keys once from `usage_bucket_30m`.
- [ ] Iterate the recomputed map by reference to update pricing fields by the
      composite primary key.
- [ ] Delete only persisted keys absent from the map using a prepared
      primary-key delete; return the deleted count.
- [ ] Remove the correlated `NOT EXISTS` query and verify the reconciliation
      helper contains no `usage_event` SQL.
- [ ] Preserve activation-meta writes and the single final commit after bucket
      update/delete.

Focused validation:

```powershell
cargo test recompute_costs_deletes_orphan_buckets --lib
cargo test pricing_bucket_reconcile --lib
```

Rollback point: revert the helper and call-site change only; no schema/data
migration exists.

## Step 2 - Store Progress And Structured Logs

- [ ] Add crate-internal bootstrap/pricing progress structs, enum, and callback
      type with version, count, phase, and elapsed fields.
- [ ] Keep `bootstrap_with_migration_events` compatible; add an internal unified
      bootstrap progress method and adapt migration events into it.
- [ ] Count total events once, emit pricing start, throttled committed-page
      progress, bucket reconcile start, and finish events.
- [ ] Return internal recompute summary fields without changing existing public
      `usize` return semantics.
- [ ] Add structured info/debug logs and a one-time warning after 30 seconds;
      log phase failures at error before preserving the original error.
- [ ] Keep log payloads free of paths, event keys, prompts, and raw catalog data.

Focused validation:

```powershell
cargo test pricing_progress --lib
cargo test paged_recompute_can_retry_after_a_later_page_fails --lib
```

Rollback point: progress callbacks and logs are observational; remove them
without reverting the linear reconciliation.

## Step 3 - Sync Human And NDJSON Events

- [ ] Add additive pricing lifecycle variants to `SyncEvent` with snake_case
      serialization.
- [ ] Create one bootstrap-to-sync conversion helper used by both human and
      JSON sync paths; remove duplicate migration mapping.
- [ ] Change the initial copy to `检查数据库 schema 与定价目录...`.
- [ ] Render catalog versions, processed/total and percentage, reconcile phase,
      deleted orphan count, and final elapsed time.
- [ ] Make TTY newline behavior explicit at reconcile/finished boundaries and
      keep non-TTY volume bounded by Store throttling.
- [ ] Audit every exhaustive `SyncEvent` match, including job-registry and test
      helpers, and deliberately handle/ignore new variants.
- [ ] Preserve stdout-only NDJSON behavior for `--json-events`.

Focused validation:

```powershell
cargo test human_progress --lib
cargo test --test m2_raw_archive_logs json_events_subprocess_emits_ndjson_per_event -- --exact
```

## Step 4 - Performance And Failure Regressions

- [ ] Expand orphan cleanup coverage to many same-source buckets and at least
      one orphan; assert event rows and live buckets remain unchanged.
- [ ] Add a structural assertion/helper boundary proving reconciliation does
      not read `usage_event`.
- [ ] Capture bootstrap progress from a static-v1 fixture and assert lifecycle
      order, monotonic counts, totals, and final static-v2 metadata.
- [ ] Cover current embedded and pinned overlay/snapshot no-op cases.
- [ ] Inject a late-page/final-phase failure and assert no finished event or
      early metadata switch; retry must converge.
- [ ] Add deterministic warning-threshold coverage without a real 30-second
      sleep.
- [ ] Extend the subprocess test to assert pricing NDJSON variants and structured
      log fields while stdout remains parseable NDJSON only.

Focused validation:

```powershell
cargo test recompute_costs --lib
cargo test pricing_catalog_upgrade --lib
cargo test --test m2_raw_archive_logs json_events -- --test-threads=1
```

## Step 5 - Contracts And User Documentation

- [ ] Update pricing-catalog contracts with linear reconciliation, progress,
      failure, and performance requirements.
- [ ] Update source-sync contracts with additive pricing lifecycle event fields
      and ordering before lock acquisition.
- [ ] Update English/Chinese README and CLI reference text for first-run embedded
      repricing and progress output.
- [ ] Update English/Chinese first-sync guides if their lifecycle examples need
      the new pricing phases.
- [ ] Keep documentation explicit that structured file info/debug logs require
      the corresponding `LLMUSAGE_LOG` level; default terminal progress does not.

Validation:

```powershell
npm --prefix docs run docs:build
rg -n "pricing_upgrade|定价目录|pricing catalog" README.md README.zh-CN.md docs .trellis/spec
```

## Step 6 - Isolated Live-Scale Verification

- [ ] Use SQLite's backup API to create a consistent temporary snapshot of
      `C:\Users\lyh\.llmusage\llmusage.db`; never run the candidate against the
      live database during this gate.
- [ ] Run the candidate binary with an isolated `--home`/`LLMUSAGE_HOME` and
      capture human output, NDJSON events, structured logs, and wall-clock phase
      timings.
- [ ] Verify bucket reconciliation is under two seconds on this machine and no
      correlated event scan appears.
- [ ] Verify `pricing_catalog_version=static-v2`, bucket/event key sets match,
      bucket pricing is consistent, and a second bootstrap emits no pricing
      upgrade events.
- [ ] Remove only the temporary snapshot created by this task after recording
      results and confirming its resolved path is outside the live home.

Rollback point: failure here blocks installation against the live database and
returns the task to the owning implementation step.

## Step 7 - Full Quality Gate And Review

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test -- --test-threads=1`
- [ ] `npm --prefix docs run docs:build`
- [ ] `git diff --check`
- [ ] Run `trellis-check`; verify performance, event ordering, log privacy,
      transaction semantics, and unrelated-worktree preservation.
- [ ] Inspect the final diff before requesting installation/live verification.

## Risk Files

- `src/store/mod.rs`: repricing pages, bucket reconciliation, final activation
  transaction.
- `src/store/schema.rs` and `src/store/pricing_catalog.rs`: bootstrap upgrade
  selection and progress plumbing.
- `src/parsers/mod.rs`: serialized `SyncEvent` compatibility.
- `src/commands/sync.rs`: human stderr, NDJSON mapping, and TTY behavior.
- `tests/m2_raw_archive_logs.rs` and pricing Store tests: cross-process and
  performance regression coverage.
- Pricing/source-sync specs and bilingual CLI docs: public behavior contract.

## Overall Rollback Strategy

No schema migration or catalog data change is planned. Revert the affected code
and event variants if required; the user's database remains compatible. Do not
repair rollback by editing `pricing_catalog_version`, deleting WAL files, or
running a lossy rebuild.
