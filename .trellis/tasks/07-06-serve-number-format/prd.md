# Optimize serve number formatting

## Goal

Make large numeric values in `llmusage serve` easier to scan by showing adaptive compact token/count labels instead of long comma-separated integers in dense dashboard rows.

## Requirements

- Scope is the web dashboard served by `llmusage serve` and reused by snapshot/export HTML assets.
- Large token values in dense distribution, trend, and insight surfaces should use adaptive units such as K, M, B, and T.
- Exact comma-separated values should remain available where the UI already uses raw/footnote/title detail, so compact display does not remove precision from the experience.
- Formatting must be centralized in the existing web asset formatter instead of duplicating ad hoc math in each renderer.
- The backend API payload and Rust query totals must remain unchanged.

## Acceptance Criteria

- [x] Model distribution bars and model table totals render compact token values for large counts.
- [x] Source and trend token rows render compact values in scan-oriented cells while preserving exact values in tooltips or existing raw-value copy.
- [x] Existing compact KPI/cost/project displays keep working through the shared formatter.
- [x] Focused tests or asset checks cover the shared compact formatter and the updated renderers.
- [x] `cargo fmt --check` and focused Rust tests pass.

## Out of Scope

- Changing backend aggregation semantics, API response fields, or stored token values.
- Redesigning dashboard layout beyond number readability.
- Changing terminal TUI number formatting.

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
