# Design

## Architecture And Boundaries

This task is a frontend shell/layout change for the browser dashboard. It should stay within:

- `src/web/shell.rs`
- `src/web/assets/layout.css`
- `src/web/assets/components.css`
- `src/web/assets/render/sync-command-center.js`
- focused tests in `src/web/mod.rs` if structure assertions need updating

The query layer, sync semantics, SQLite data, and API payload contracts should not change.

## Current Problem

The current first viewport prioritizes the sync command center as a large dark instrument card before usage data. On the docs fixture at `1440x1100`, the command center is 545px tall and pushes the KPI strip mostly below the fold. Trends start below the first viewport.

This conflicts with the dashboard documentation's first-screen workflow, which prioritizes filters, KPI, and trend review.

## Product Direction

Keep sync status visible and actionable, but reduce its default footprint. The first viewport should answer the user's main dashboard question first: "where did my tokens/cost go?" Sync status should explain data freshness and allow a sync action without dominating the page.

The user approved this usage-first direction during planning.

## Proposed Layout

- Move or restyle sync status as a compact strip inside `#overview`, after the primary usage summary and before or near the filters, not as a full hero-height first card.
- Preserve the current structured data contract and button delegation to `#btn-sync`.
- Keep KPI cards visible above the fold on desktop fixture size.
- Let trends begin within or just after the first desktop viewport.
- On mobile, reduce pre-content height by tightening sidebar/topbar behavior and ensuring the first dashboard content appears sooner.

## Design Constraints

- Preserve the existing warm paper + dark instrument identity.
- Do not introduce a new palette or decorative gradients.
- Keep terracotta as the only brand accent.
- Keep numeric data in monospace.
- Do not use side-stripe borders, gradient text, oversized radii, or decorative shadow stacks.
- Keep sync risk and failure signals explicit through text plus semantic color.

## Compatibility

The existing sync command center payload stays unchanged. The renderer can change presentation, but must continue to:

- avoid parsing human `summary` strings
- avoid exposing raw error paths or tokens
- use `error_key` / copy keys
- delegate action clicks through `#btn-sync`
- handle active job overlays

## Validation

- Re-run the docs fixture.
- Capture desktop and mobile screenshots before and after.
- Measure first viewport section bounds.
- Run targeted Rust tests around web shell/assets.
- Run formatting and broader gates if practical.
