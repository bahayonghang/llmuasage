# vibeusage Quality Guidelines

## Local Commands

Use package scripts from `package.json` and project guidance:

- `npm run ci:local`
- `npm run build:insforge:check`
- `npm --prefix dashboard run build`
- `npm run validate:copy`
- `npm run validate:ui-hardcode`
- `npm run validate:guardrails`
- `npm run validate:retros`

## Source Integration Checklist

For any new AI CLI token usage source, `AGENTS.md` requires:

- a documented dedupe key, or an explicit reason no upstream unique id exists,
- `total_tokens` equal to all available token channels,
- real-session fixtures covering per-channel totals, dedupe/re-run safety, and
  cache-read-heavy regressions when the source supports cache reads,
- dashboard client visibility updates.

Reference tests include `ref/vibeusage/test/rollout-parser.test.js`.

## InsForge And PostgREST

- Edge functions are authored from `insforge-src/`.
- PostgREST aggregate queries use `sum(column)`, not `column.sum()`.
- New or changed aggregate endpoints require a real InsForge smoke check when
  the environment is available.

## Copy And Sitemap

- User-facing text changes require copy registry updates and validation.
- Boundary, route, or preferred-entry changes require `docs/repo-sitemap.md`
  updates.

## Avoid

- Do not skip `npm run ci:local` for broad source/dashboard changes.
- Do not add a source based only on docs without real fixture coverage.
- Do not ignore `schema cache` or `sum` errors; route them through aggregate
  fallback logic and record the root cause.
