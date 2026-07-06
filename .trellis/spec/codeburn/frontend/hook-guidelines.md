# codeburn Hook Guidelines

## Ink Hooks

Use hooks where the existing Ink views already do:

- `useInput()` handles keyboard navigation, tab switching, period changes, and
  quitting.
- `useEffect()` starts async parsing, provider discovery, or derived scans.
- `useRef()` stores latest async inputs or mounted-state guards to avoid stale
  closures.
- `useState()` owns local view state such as period, provider, active tab,
  loading, and reload counters.

Reference files:

- `ref/codeburn/src/dashboard.tsx`
- `ref/codeburn/src/compare.tsx`

## Async And Native Dependencies

- Keep expensive provider scans in effects or command-level async flows.
- Lazy-load heavy native dependencies through `src/providers/index.ts`.
- `src/sqlite.ts` loads `node:sqlite` lazily and only silences the known SQLite
  experimental warning, not every process warning.

## Avoid

- Do not call provider discovery during render.
- Do not add static imports of heavy native modules to universal CLI startup.
- Do not leave async effects without a stale-result guard when the view can
  reload or change period/provider while work is in flight.
