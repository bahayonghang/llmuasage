# Production D1 validation

Captured: `2026-07-30`

## Implemented boundary

- Schema v19 adds only
  `idx_usage_event_activity_cost(event_key, cost_with_cache_usd)`.
- Activity reducer/query/filter semantics, cache, concurrency, frontend,
  PERF-002 supervision, and the 3-second Behavior deadline are unchanged.
- The schema-v18 migration regression is isolated with `MIGRATIONS[..18]`.

## Focused evidence

- `migration_v19_upgrades_v18_and_matches_fresh_schema`: passed. Both upgrade
  and fresh paths reach v19, retain the exact two index columns, and expose the
  covering-index plan.
- `activity_serialization_is_identical_before_and_after_v19_index`: passed.
  Serialized bytes match the legacy oracle and the pre-index result for missing
  events, NULL cost, `edit_turns=0`, category ties, and source/model/project/
  date/no-data filters.
- Explicit single-thread sync acceptance benchmark: passed over seven
  alternating rounds of fixed 4,000-event shards. Baseline median was
  `166.968 ms`; indexed median was `172.029 ms`; ratio was `1.030314`, a
  `3.03%` regression below the `10%` blocking threshold.

## Full gates

- `python scripts/ci-rust.py`: passed, including Clippy, 572 library tests,
  integration tests, rustdoc, and formatting.
- `just ci`: passed, including the shared Rust gate, dashboard Node checks/tests,
  and VitePress build.
- Task context validation, `cargo fmt --check`, `git diff --check`, Ruff, and
  Pyright (`0 errors`) passed.
- The `trellis-check` agent dispatch could not be established by the available
  collaboration adapter. The main session therefore ran the required full-scope
  check against the PRD/design/implementation, migration runner, write-fencing
  contract, dashboard performance contract, affected-schema references, and
  unrelated dirty-file boundary; no blocking finding remained.

## Decision

Production D1 automated acceptance is `PASS`. D2 remains out of scope. Final
cold-start acceptance is pending the sealed v19 snapshot reboot protocol.
