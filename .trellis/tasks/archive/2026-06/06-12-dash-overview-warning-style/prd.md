# Optimize dash overview and CLI warnings

## Goal

Make the `llmusage dash` overview screen more useful at first glance and make
the deprecated `llmusage tui` warning visually consistent with the rest of the
CLI.

## Requirements

- The Overview panel must use the existing `OverviewPayload`; do not add a new
  dashboard query, SQLite migration, or cross-layer payload contract for this
  task.
- The Overview panel should reduce the empty first screen by adding compact
  summaries derived from current totals, last-24-hour totals, event counts,
  cache ratio, and freshness timestamps.
- The Overview panel must still render safely on narrow and short terminals.
- `llmusage tui` must continue to work as a hidden deprecated alias for
  `llmusage dash`.
- The deprecated alias warning must use ANSI styling when color is enabled and
  keep plain text output when color is disabled.
- Warning color behavior must follow the existing CLI color environment rules:
  `LLMUSAGE_FORCE_COLOR` / `CLICOLOR_FORCE` force color, while `NO_COLOR` /
  `LLMUSAGE_NO_COLOR` disable it.

## Acceptance Criteria

- [x] `llmusage dash` Overview renders KPI cards plus additional summary
      sections instead of only four cards and two metadata lines.
- [x] Overview tests assert the new summary labels and values are present.
- [x] `llmusage tui` warning includes ANSI escape sequences under forced color.
- [x] `llmusage tui` warning remains plain text when color is disabled.
- [x] `llmusage dash` emits no deprecated alias warning.
- [x] Focused Rust tests for `dash` command and TUI overview pass.
- [x] `cargo fmt --check` passes.

## Notes

- User-provided screenshot showed a large terminal where Overview only used the
  top strip, leaving most of the first screen blank.
