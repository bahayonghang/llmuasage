# Implement

## Ordered Checklist

- [x] Confirm the schema-migration risk decision in `prd.md`: a forward-only
      migration is allowed when query-plan and read/write measurements justify
      each index.
- [x] Review and approve the numeric budgets before task activation.
- [x] Load `trellis-before-dev` and the relevant llmusage backend guidelines.
- [x] Add an agent-runnable browser/HTTP benchmark for one click and rapid
      1d -> 7d -> 30d -> all switching; capture click feedback, request count,
      payload bytes, critical render, secondary completion, and long tasks.
- [x] Add structured server timing around semaphore wait, query execution,
      serialization, section, scope, and cancellation.
- [x] Introduce the additive interactive dashboard projection with only the
      selected trend and a health summary (`cursor_count`, no cursor rows).
- [x] Add contract tests proving full/snapshot compatibility and enforcing the
      interactive payload-size/field boundary without flaky wall-clock CI
      assertions.
- [x] Refactor range refresh to synchronously update control/loading state and
      render the lean interactive response without rerendering unchanged
      secondary panels.
- [x] Add generation-scoped `AbortController` support through
      `loadJson`/`loadLiveJson`; bound the cache and ensure invalidation aborts
      rather than merely forgetting in-flight work.
- [x] Replace the background full dashboard request with bounded independent
      secondary requests and merge results only when generation/filter match.
- [x] Add browser/request-lifecycle tests for rapid switching, latest-wins,
      intentional abort, cache coalescing, stale labels, and per-section error
      isolation.
- [x] Add a cancellable SQLite blocking-query wrapper using rusqlite
      `InterruptHandle`; prove timeout/client cancellation terminates work and
      releases the semaphore permit.
- [x] Route compatible Explorer queries through `usage_bucket_30m` and add
      event-vs-bucket equivalence tests for totals, ranking, Other, series,
      filters, and timezone boundaries.
- [x] Re-run query plans. No standalone time index remained justified after
      aggregate routing and source-indexed latest-event lookups; therefore no
      schema migration was added. If a future fact path regresses, require the
      original plan/timing/write-cost gate before adding one.
- [x] Re-run the representative 523k-event baseline for all four ranges and a
      rapid-switch stress loop; compare median/p95, bytes, request count, and
      abandoned-work duration with the planning baseline.
- [x] Run targeted tests, `cargo fmt --check`, `cargo clippy --all-targets
      --all-features -- -D warnings`, `cargo test -- --test-threads=1`, docs
      build if docs change, then `just ci`.
- [x] Update the relevant `.trellis/spec/llmusage/backend/` contract with the
      interactive payload, aggregate Explorer routing, and cancellation rules.

## Validation Commands

```powershell
cargo test web::tests:: -- --test-threads=1
cargo test query::explorer -- --test-threads=1
cargo test store::migrations -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --test-threads=1
npm --prefix docs run docs:build
just ci
git diff --check
```

The implementation should add a checked-in benchmark command/script. Until
then, preserve the planning baseline command shape:

```powershell
$ranges = '1d','7d','30d','all'
foreach ($range in $ranges) {
  Invoke-WebRequest -UseBasicParsing `
    -Uri "http://127.0.0.1:37422/api/dashboard?range=$range&scope=interactive"
}
```

## Review Gates

1. **Contract gate:** interactive response contains no cursor rows, returns one
   selected trend, and full/export responses remain equivalent.
2. **Lifecycle gate:** rapid clicks cannot publish stale data and obsolete work
   is physically cancelled, not only ignored at render time.
3. **Query gate:** Bucket strategy is selected only for compatible semantics;
   fallback plans are explicit and covered.
4. **Migration gate:** no index migration without before/after plans, read
   timings, migration timing, and sync write-cost evidence.
5. **End-to-end gate:** browser p95 and representative HTTP p95/bytes meet the
   PRD budgets for all four ranges.

## Risky Files And Rollback Points

- `src/web/assets/app.js`: checkpoint after immediate feedback and latest-wins.
- `src/web/assets/data/fetch.js`: checkpoint after abort/cache tests.
- `src/web/mod.rs`: checkpoint after interactive/full contract tests.
- `src/query/explorer.rs`: checkpoint after event-vs-bucket golden tests.
- `src/store/migrations.rs`: last and optional checkpoint; revert independently
  if write/migration cost is not justified.

## Baseline Evidence

Captured 2026-07-11 against the current live local database:

| Surface | Baseline |
| --- | ---: |
| core 1d/7d/30d/all | 639-839 ms, 1.71-1.76 MB |
| full 1d | 2.00-2.15 s |
| full 7d | 2.33-2.69 s |
| full 30d | 2.57-2.72 s |
| full all | 4.04-5.38 s |
| health payload | 1.69 MB, 7,382 cursor rows |
| Explorer 7d | 1.66 s |
| event source/day cost SQL 7d | 805-1,005 ms, full event scan |
| bucket count SQL 7d | under 1 ms, indexed seek |

The in-app browser was unavailable during planning, so click-to-paint and long
task baselines are the first required implementation checkpoint rather than an
assumed number.

## Final Evidence

Captured 2026-07-11 with five post-warm-up samples per range against the same
523,644-event representative database. Full details are in
`benchmark-final.json`.

| Surface | Final result |
| --- | ---: |
| interactive p95 1d / 7d / 30d / all | 142.16 / 156.40 / 160.57 / 238.98 ms |
| interactive max bytes 1d / 7d / 30d / all | 15,714 / 19,316 / 25,941 / 66,450 B |
| browser click feedback max | 3.7 ms |
| browser critical render 1d / 7d / 30d / all | 282.0 / 316.8 / 186.5 / 239.2 ms |
| rapid-switch latest response | 202.9 ms, active preset `all` |
| rapid-switch interactive requests | 4, one per user click |

Validation passed with `node --test scripts/tests/dashboard-fetch.test.mjs`,
targeted Rust regressions, `git diff --check`, and `just ci` (CI performance
threshold enabled; external `CARGO_TARGET_DIR` used to avoid locking the user's
running dashboard executable on Windows).
