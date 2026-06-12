# Implementation Plan

1. Update warning formatting.
   - Add a stderr color check to `ColorMode`.
   - Update `dash.rs` deprecated warning rendering and unit tests.
   - Verify with `cargo test commands::dash`.

2. Expand Overview rendering.
   - Keep KPI cards.
   - Add fixed-height summary sections using only `OverviewPayload`.
   - Keep narrow terminal fallback.
   - Verify with focused TUI overview tests.

3. Run validation.
   - `cargo fmt --check`
   - Focused tests for command warning and overview panel.

## Rollback Points

- If overview layout becomes brittle on narrow terminals, keep only the KPI row
  plus stacked text summaries.
- If warning styling conflicts with color mode tests, retain the plain message
  and isolate styling behind `ColorMode::Always` / `Never` tests first.
