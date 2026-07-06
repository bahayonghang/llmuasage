# TokenTracker Component Guidelines

## Pages And Layout

Dashboard routes are wired in `dashboard/src/App.jsx`. Most page modules are
lazy-loaded with `React.lazy()`, then wrapped in `AppLayout` where appropriate.

Local layout rule from `CLAUDE.md`:

- Pages inside `AppLayout` use `flex flex-col flex-1` as the outer wrapper.
- Do not replace that with page-local `min-h-screen` and duplicate sticky
  header/footer behavior.

Reference pages include `LimitsPage.jsx`, `LeaderboardPage.jsx`, and
`SettingsPage.jsx`.

## UI Primitives

Reuse existing primitives and styling conventions before creating new ones:

- `dashboard/src/ui/components/Button.jsx`
- `dashboard/src/ui/components/Card.jsx`
- dashboard-specific components under `dashboard/src/ui/dashboard/components/`

Keep component props focused on rendered state and callbacks; derive data in
page or hook layers.

## Copy

All user-facing text belongs in `dashboard/src/content/copy.csv` and is read
through `copy()` from `dashboard/src/lib/copy.ts`. Tests and validators enforce
this convention.

## Special Component Contract

`NativeAuthCallbackPage` must stay eagerly imported in `App.jsx`. Its module
captures the `insforge_code` query parameter before the SDK removes it from the
URL. Converting this page to `React.lazy()` breaks OAuth callback handling.

## Avoid

- Do not hardcode visible text in JSX.
- Do not create a new component when an existing Button/Card/page pattern fits.
- Do not move OAuth callback capture into a lazy route.
