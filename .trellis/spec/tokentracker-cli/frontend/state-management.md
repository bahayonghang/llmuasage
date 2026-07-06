# TokenTracker State Management

## Usage Queue Semantics

The queue contract in `CLAUDE.md` is central:

- Queue entries use UTC half-hour buckets.
- Readers take the latest entry per `(source, model, hour_start)`.
- `total_tokens` is the sum of all token columns.
- Cost is computed from individual token channels, never from `total_tokens`.
- The project stores token counts only, not prompts, messages, or conversation bodies.

Reference files:

- `ref/TokenTracker/CLAUDE.md`
- `ref/TokenTracker/src/lib/pricing/index.js`

## Dashboard State

Keep page state close to the page that owns the workflow:

- `DashboardPage.jsx` orchestrates hooks, derived props, and view selection.
- Data hooks own loading, error, local mode, and cache fallback details.
- Reusable components receive normalized props and callbacks.

## Cache And Fallback State

Use hook-level cache fallback instead of component-level retry state. If a
change affects cache keys or local/mock mode, update the hook tests and the
architecture guardrails.

## Avoid

- Do not compute cost from `total_tokens`.
- Do not spread token-channel formulas across components.
- Do not store raw conversation content in queue, cache, API payloads, or tests.
