# vibeusage State Management

## Provenance Contract

Use the provenance meanings from `docs/repo-sitemap.md`:

- `edge` means successful backend truth, including immediate reuse of the last
  successful live snapshot while a refresh is in flight.
- `cache` means browser-local fallback only after the current backend refresh
  fails.
- `mock` means mock mode only.

Do not relabel cached or snapshot-backed data as mock.

## Model Identity

- Use `model_id` as the canonical key for filtering, pricing, and aggregation.
- Use `display_model` only for presentation.
- Public APIs may expose `display_model` as response-only display data derived
  from `model_id` or `model`.

## Dashboard State

- `App.jsx` owns auth gate, session state, and route selection.
- `DashboardPage.jsx` owns view-level controls such as period, public view, and
  keyboard shortcuts.
- Hooks own loading, refreshing, cache fallback, and live snapshot reuse.

## Avoid

- Do not store model filters against display labels.
- Do not invent a second provenance vocabulary in components.
- Do not let browser cache be treated as backend truth after refresh failure
  unless provenance is `cache`.
