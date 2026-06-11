# Optimize serve dashboard whitespace

## Goal

Reduce excessive blank space in the `llmusage serve` browser dashboard while preserving the existing local-first instrument design system.

## User Value

When a developer opens the local dashboard, the first viewport should show meaningful usage context and controls without requiring unnecessary scrolling through empty regions.

## Confirmed Facts

- The user reports that the current `llmusage serve` page has large blank areas.
- The surface is a product UI dashboard, not a marketing page.
- The project design contract exists and emphasizes a calm local-first instrument: warm paper surface, restrained terracotta accent, dark instrument data cards, monospace numeric data, and no generic SaaS dashboard treatment.
- The repository is clean on branch `dev` at task creation.
- The docs screenshot fixture is the intended sanitized reproduction path: `cargo run --features testing --example docs_dashboard_serve -- --port 37421`.
- Browser verification against the fixture at `http://127.0.0.1:37421` captured screenshots in `output/playwright/serve-before-1440x1100.png` and `output/playwright/serve-before-390x844.png`.
- Desktop measurement at `1440x1100`: `#overview` is 1055px tall, `#sync-command-center` is 545px tall, `#kpi-grid` is only 44% visible in the first viewport, and `#trends` is fully below the fold.
- Switching the fixture to the `all` range populates real usage data but does not change the `#overview` / `#sync-command-center` heights, so the primary issue is layout priority rather than empty data alone.
- The dashboard docs say the first-screen workflow starts with filters, KPI strip, and trend chart; the current shell renders the sync command center before the hero, filters, KPI, and trends.
- Mobile screenshot at `390x844` shows the full sidebar and topbar consume substantial height before dashboard content begins.

## Requirements

- Reproduce and verify the whitespace issue in a real browser against the running `llmusage serve` dashboard.
- Identify whether the blank space is caused by layout sizing, missing/empty data states, responsive behavior, or unintended rendering gaps.
- Improve the dashboard layout using the existing design tokens and component vocabulary.
- Prioritize usage data in the first viewport: filters, KPI cards, and trend context should appear earlier than the expanded sync command center.
- Keep sync status visible and actionable, but present it as a compact status/action surface unless a running job or risky state needs more detail.
- Preserve the bilingual, theme-aware, local-first dashboard behavior.
- Keep changes scoped to the dashboard whitespace/design issue and its tests or verification artifacts.

## Acceptance Criteria

- [x] Browser screenshots or measurements show the whitespace problem before changes.
- [x] The updated dashboard makes better use of the first viewport on desktop and mobile widths without crowding the UI.
- [x] No text overlaps or overflows in the checked viewports.
- [x] Existing design constraints from PRODUCT.md / DESIGN.md are preserved.
- [x] Targeted frontend/static checks pass; broader repo gates are run when practical.

## Out Of Scope

- Changing usage calculations, storage, parser behavior, or sync semantics.
- Rebranding the dashboard or introducing a new palette.
- Adding new analytics features unrelated to whitespace and information hierarchy.

## Open Questions

- None. The user approved the usage-first direction: KPI / filters / trends should be prioritized, while sync status remains visible as a compact status/action surface.

## Notes

- After screenshots were captured in `output/playwright/serve-after-1440x1100.png` and `output/playwright/serve-after-390x844.png`.
- After DOM measurements at `1440x1100`: no horizontal overflow; `#kpi-grid` is 100% visible; compact `#sync-command-center` is 233px tall and fully visible; `#trends` starts in the first viewport with 22% visible.
- After DOM measurements at `390x844`: no horizontal overflow; mobile sidebar is 53px tall; `#kpi-grid` starts at 375px and is 75% visible in the first viewport; sync details remain collapsed by default.
- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
