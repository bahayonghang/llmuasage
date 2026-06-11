# vibeusage Hook Guidelines

## App Auth Hooks

`dashboard/src/App.jsx` owns auth/session restoration:

- It uses `useSyncExternalStore` with InsForge session subscription helpers.
- It tracks `sessionExpired` state.
- It resolves auth gates and public/developer-resource routes.
- A guardrail test asserts important hook wiring remains present.

Reference files:

- `ref/vibeusage/dashboard/src/App.jsx`
- `ref/vibeusage/test/dashboard-react-hooks-guardrails.test.js`

## Usage Data Hooks

Keep fetch and cache logic in hooks:

- `use-usage-data.ts` manages cache keys, live snapshot reuse, abortable fetches,
  loading versus refreshing, and provenance labels.
- `use-usage-model-breakdown.ts` hydrates display models, reuses cache/live
  snapshots, and aborts stale fetches.

Use `AbortController` for refreshes that can be superseded by route, period, or
auth changes.

## Page Orchestration

`DashboardPage.jsx` should orchestrate hooks, keyboard shortcuts, period changes
with `startTransition`, public view state, and copy lookups. Leaf components
should not own fetch policy.

## Avoid

- Do not duplicate API fetch logic inside components.
- Do not conflate initial loading with background refreshing.
- Do not let stale async responses overwrite newer period/auth state.
