# TokenTracker Hook Guidelines

## Data Hooks

Dashboard data loading is centralized in hooks:

- `dashboard/src/hooks/use-usage-data.ts` fetches daily and summary data,
  supports local/mock modes, fills daily gaps, and falls back to local cache.
- `dashboard/src/hooks/use-project-usage-summary.ts` guards on access token and
  local mode before fetching project usage.
- Page modules such as `DashboardPage.jsx` orchestrate hooks and pass derived
  props to presentational views.

Keep fetch, fallback, and normalization logic in hooks or API helpers, not in
leaf components.

## Route And Auth Hooks

`dashboard/src/App.jsx` uses `useMemo` and lazy route elements to avoid
rebuilding page nodes unnecessarily. Preserve the eager import exception for
`NativeAuthCallbackPage`.

## Derived Props

Use `useMemo` and `useCallback` for expensive derived props and stable handlers
when the page already follows that pattern. Do not add memoization only as a
style preference; use it when it prevents repeated copy lookup, expensive
aggregation, or child remounts.

## Avoid

- Do not fetch directly from deeply nested UI components.
- Do not let hooks collect prompts, messages, or conversation bodies; the
  project is token-count-only.
- Do not trust a subagent summary for hook changes; verify changed files with
  direct reads, per `CLAUDE.md` lessons learned.
