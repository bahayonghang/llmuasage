# Implementation plan

## Checklist

- [ ] Turn the current HTTP red loop into a focused automated web/query regression that asserts the exact timeout symptom before the fix.
- [ ] Add migration v18 and tests for v17 drift repair plus fresh bootstrap/index parity.
- [ ] Change behavior support probing from `COUNT(*)` to `EXISTS` with no support-level changes.
- [ ] Implement Activity sequential aggregation and retain a test-only legacy SQL oracle for field/order equivalence.
- [ ] Implement Tools typed sequential attribution with exact filtered-tool/non-tool/orphan/distinct semantics and oracle coverage.
- [ ] Rewrite Optimize low-read/edit and session-outlier queries; cover positive, negative, and filtered findings.
- [ ] Batch Compare queries for both selected models and prove complete payload equivalence.
- [ ] Apply the three-second Behavior-specific hard deadline while preserving general API, cancellation, permit, and degraded contracts.
- [ ] Run focused query, migration, and web tests; inspect `EXPLAIN QUERY PLAN` for final index use.
- [ ] Before live-current-database bootstrap, create a separate SQLite online backup and verify that it opens read-only as schema v17.
- [ ] Rebuild the debug/release binary, start a task-owned loopback server, and run three `1d` plus three `all` section rounds and a concurrency-2 round against the current database.
- [ ] Verify the Behavior DOM in a real browser contains no timeout reason and the four support tags/data sections settle.
- [ ] Run `cargo fmt --check`, targeted clippy/tests, `python scripts/ci-rust.py`, then `just ci`; inspect final diff and remove temporary instrumentation.

## Focused validation

```powershell
cargo test --lib migrations::tests -- --test-threads=1
cargo test --lib query::tests::behavior -- --test-threads=1
cargo test --lib web::tests::behavior -- --test-threads=1
python scripts/ci-rust.py
just ci
```

Exact test filters will be adjusted to the final test names; a zero-test filter is not completion evidence.

## Measurement gate

- Dataset scale and output remain aggregate-only; do not persist paths, sessions, model names, or response bodies.
- Before the first current-database v18 bootstrap, create a separate SQLite online backup and verify its schema version and integrity in read-only mode.
- Measure each section independently and through browser concurrency two.
- Required result: no deadline degradation in any sample; `1d` three-sample median below one second; `all` every section below the three-second hard ceiling.

## Rollback points

- Before v18: if the final read path does not use a proposed index, omit that index rather than accepting write amplification.
- Before removing legacy SQL: require exact fixture equivalence across all filter and attribution edge cases.
- Before changing the deadline: require measured query improvement. Do not ship a constant-only change.
- If Tools cannot preserve exact semantics within the ceiling, return to planning for a versioned Behavior rollup design.
