# TokenTracker Directory Structure

## Source Of Truth

`AGENTS.md` points to `CLAUDE.md` as the single source of truth. Read
`ref/TokenTracker/CLAUDE.md` before changing repository conventions.

## Runtime Areas

- `src/` is the CommonJS CLI runtime. `bin/tracker.js` is the executable entry.
- `dashboard/` is a React 18 + Vite 7 + strict TypeScript dashboard.
- `TokenTrackerBar/` is the Swift menu bar app and WidgetKit surface.
- `TokenTrackerBar/EmbeddedServer/` bundles the CLI runtime and built dashboard.
- `dashboard/src/content/copy.csv` is the copy registry for user-facing text.

Reference files:

- `ref/TokenTracker/CLAUDE.md`
- `ref/TokenTracker/package.json`
- `ref/TokenTracker/dashboard/package.json`

## Dashboard Paths

- Add pages under `dashboard/src/pages/`.
- Add shared UI components under existing dashboard UI directories; reuse simple
  primitives such as `dashboard/src/ui/components/Button.jsx` and `Card.jsx`.
- Keep route registration and lazy loading in `dashboard/src/App.jsx`.
- Add provider icons through `ProviderIcon.jsx` using the existing provider-key
  map convention.

## Cross-Surface Rule

Changes under `src/` or `dashboard/` affect both npm and the DMG because the
menu bar app embeds those assets. Treat dashboard changes as release-impacting,
not web-only.

## Avoid

- Do not mix CommonJS `src/` patterns into the ESM/TypeScript dashboard.
- Do not hardcode UI strings outside `copy.csv`.
- Do not edit the Homebrew tap manually for routine releases.
