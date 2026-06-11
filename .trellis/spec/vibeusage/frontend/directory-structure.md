# vibeusage Directory Structure

## Navigation Source

For non-trivial work, read `ref/vibeusage/docs/repo-sitemap.md` first. It is
the repository navigation source of truth. Update it when a change affects
module boundaries, cross-module data flow, public contracts, or preferred entry
files.

## Runtime Areas

- `src/` is the Node.js CommonJS CLI source.
- `dashboard/` is the React/Vite dashboard.
- `insforge-src/` is the authoritative source for live `vibeusage-*` InsForge
  edge functions.
- `test/` contains Node test coverage for CLI, edge functions, scripts, and
  regressions.
- Generated function artifacts are for deploy validation and deployment, not
  authoring.

Reference files:

- `ref/vibeusage/AGENTS.md`
- `ref/vibeusage/CLAUDE.md`
- `ref/vibeusage/docs/repo-sitemap.md`

## Dashboard Paths

The sitemap identifies key dashboard areas:

- `dashboard/src/main.jsx`
- `dashboard/src/App.jsx`
- `dashboard/src/pages/`
- `dashboard/src/hooks/`
- `dashboard/src/ui/matrix-a/components/`
- auth/session helpers in `dashboard/src/lib/insforge-auth-client.ts`,
  `insforge-client.ts`, and `vibeusage-api.ts`

## Avoid

- Do not author against retired legacy paths called out by the sitemap.
- Do not update generated edge artifacts as the primary source.
- Do not change route/data boundaries without updating `docs/repo-sitemap.md`
  when the sitemap contract is affected.
