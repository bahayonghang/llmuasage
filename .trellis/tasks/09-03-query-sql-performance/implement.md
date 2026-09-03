# Implement: query SQL performance

1. Add failing tests: activity SQL must not be `FROM usage_event` without WHERE; session `--id` EXPLAIN/SQL contains session bind; source breakdown issues one grouped MAX.
2. Switch `activity_breakdown` to filtered join.
3. Rewrite `home_overview` load/load_compact aggregations.
4. Switch `top_sessions` production path to grouped SQL.
5. Push session id into reports SQL; keep `visit_filtered_events` for tests or duration-only if still required.
6. Fold last_event_at and TUI context_pressure.
7. Collapse overview bucket scans if cheap; skip if it risks JSON drift.
8. Fix HOME_PLATFORMS.
9. Run `cargo test --all-features query -- --test-threads=1` and `tests/query`, `tests/tui` slices.

Validation: `python scripts/ci-rust.py` if time; otherwise focused query+tui+cli report tests.

Rollback: git revert the query commits.
