# vibeusage Frontend Guidelines

`ref/vibeusage` combines a CommonJS CLI, a React/Vite dashboard, and InsForge
edge functions. The frontend layer is the dashboard plus the contracts it
consumes from local sync, browser cache, and backend/edge APIs.

## Pre-Development Checklist

- Read [Directory Structure](./directory-structure.md) before changing dashboard, CLI, edge, or docs boundaries.
- Read [Component Guidelines](./component-guidelines.md) before editing dashboard pages, Matrix UI components, copy, or model labels.
- Read [Hook Guidelines](./hook-guidelines.md) before changing auth restore, usage data hooks, abort behavior, or live snapshot reuse.
- Read [State Management](./state-management.md) before changing provenance, model keys, cache fallback, or route state.
- Read [Quality Guidelines](./quality-guidelines.md) before changing sources, guardrails, copy, InsForge queries, or builds.
- Read [Type Safety](./type-safety.md) before changing API payloads, model fields, token totals, or PostgREST aggregates.
- Also read `.trellis/spec/guides/index.md` for shared cross-layer and reuse checks.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | CLI, dashboard, InsForge source, sitemap | Documented |
| [Component Guidelines](./component-guidelines.md) | Matrix UI, copy registry, display model rules | Documented |
| [Hook Guidelines](./hook-guidelines.md) | Auth session restore, abortable fetches, snapshots | Documented |
| [State Management](./state-management.md) | edge/cache/mock provenance and canonical model keys | Documented |
| [Quality Guidelines](./quality-guidelines.md) | ci:local, source checklist, guardrails, InsForge smoke | Documented |
| [Type Safety](./type-safety.md) | CommonJS/ESM boundaries, token totals, aggregate syntax | Documented |

## Quality Check

- For spec-only changes, scan `.trellis/spec/vibeusage/frontend/` for template
  markers and trailing whitespace.
- For dashboard, CLI, or edge changes, run the relevant scripts from
  `quality-guidelines.md`, with `npm run ci:local` as the broad local gate.
- For source or aggregate changes, verify the source checklist, PostgREST
  aggregate syntax, and any available InsForge smoke evidence.

## Core References

- `ref/vibeusage/AGENTS.md`
- `ref/vibeusage/CLAUDE.md`
- `ref/vibeusage/docs/repo-sitemap.md`
- `ref/vibeusage/package.json`
- `ref/vibeusage/dashboard/src/App.jsx`
- `ref/vibeusage/dashboard/src/content/copy.csv`
- `ref/vibeusage/dashboard/src/hooks/use-usage-data.ts`
- `ref/vibeusage/dashboard/src/hooks/use-usage-model-breakdown.ts`
- `ref/vibeusage/dashboard/src/pages/DashboardPage.jsx`
- `ref/vibeusage/test/dashboard-react-hooks-guardrails.test.js`

All vibeusage spec documentation is written in English.
