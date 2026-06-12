# Remaining TODO

## Scope Decisions

- [x] Keep missing platforms monitor-only unless real sanitized fixtures and token semantics exist.
- [x] Treat Antigravity as integration/monitoring only until token-bearing local artifacts are documented.
- [x] Keep the TUI on existing Dashboard/query/sync contracts; do not add parser or scanner logic to rendering.

## Completion Checklist

- [x] Create `research/tokscale-matrix.md` with parsed vs monitor-only actions.
- [x] Expose skipped unchanged artifacts and parsed/committed counts in sync stats/output.
- [x] Add focused sync tests for unchanged second sync and rewrite invalidation visibility.
- [x] Add TUI affordances for sync action visibility, theme/settings status, and Daily detail context.
- [x] Update user docs for monitored vs parsed platforms, token quality, cache/fingerprint behavior, and dash controls.
- [x] Run targeted validation plus `just ci`.
- [ ] Record final task notes, decide whether spec updates are needed, commit, and finish/archive.
