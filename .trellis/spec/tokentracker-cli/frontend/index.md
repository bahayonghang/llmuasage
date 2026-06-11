# TokenTracker Frontend Guidelines

`ref/TokenTracker` has three coupled surfaces: a CommonJS CLI, a React/Vite
dashboard, and a Swift menu bar app that embeds the CLI runtime plus built
dashboard. This frontend spec focuses on the dashboard and its contracts with
local/cloud APIs.

## Pre-Development Checklist

- Read [Directory Structure](./directory-structure.md) before changing CLI, dashboard, or native boundaries.
- Read [Component Guidelines](./component-guidelines.md) before editing pages, reusable UI, copy, or layout.
- Read [Hook Guidelines](./hook-guidelines.md) before changing dashboard data loading or route/auth behavior.
- Read [State Management](./state-management.md) before changing queue semantics, cache fallback, or page orchestration.
- Read [Quality Guidelines](./quality-guidelines.md) before changing validation, tests, build, privacy, or release-impacting files.
- Read [Type Safety](./type-safety.md) before changing token fields, cost math, module systems, or API payloads.
- Also read `.trellis/spec/guides/index.md` for shared cross-layer and reuse checks.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | CLI, dashboard, native app, copy registry | Documented |
| [Component Guidelines](./component-guidelines.md) | Pages, layouts, UI primitives, copy lookup | Documented |
| [Hook Guidelines](./hook-guidelines.md) | Dashboard data hooks, route lazy loading, auth callback | Documented |
| [State Management](./state-management.md) | Queue buckets, local cache, page-derived state | Documented |
| [Quality Guidelines](./quality-guidelines.md) | ci:local, copy validation, guardrails, privacy | Documented |
| [Type Safety](./type-safety.md) | Token channel semantics, cost math, module boundaries | Documented |

## Quality Check

- For spec-only changes, scan `.trellis/spec/tokentracker-cli/frontend/` for
  template markers and trailing whitespace.
- For dashboard or CLI changes, run the relevant scripts from
  `quality-guidelines.md`, with `npm run ci:local` as the broad local gate.
- For UI copy changes, include `npm run validate:copy` and
  `npm run validate:ui-hardcode`.

## Core References

- `ref/TokenTracker/AGENTS.md`
- `ref/TokenTracker/CLAUDE.md`
- `ref/TokenTracker/package.json`
- `ref/TokenTracker/dashboard/src/App.jsx`
- `ref/TokenTracker/dashboard/src/content/copy.csv`
- `ref/TokenTracker/dashboard/src/hooks/use-usage-data.ts`
- `ref/TokenTracker/dashboard/src/hooks/use-project-usage-summary.ts`
- `ref/TokenTracker/dashboard/src/pages/DashboardPage.jsx`
- `ref/TokenTracker/dashboard/src/ui/components/Button.jsx`
- `ref/TokenTracker/dashboard/src/ui/components/Card.jsx`

All TokenTracker spec documentation is written in English.
