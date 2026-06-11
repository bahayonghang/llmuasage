# vibeusage Type Safety

## Module Boundaries

- Root `src/` uses CommonJS.
- The dashboard is React/Vite.
- Root CommonJS entrypoints load the official InsForge SDK through async
  `import("@insforge/sdk")`; direct `require("@insforge/sdk")` is not the
  supported path.
- InsForge ESM function source lives under `insforge-src/`.

## Token Totals

New source normalizers must make `total_tokens` include every available token
channel, including cache read, cache creation, and reasoning channels. Tests in
`rollout-parser.test.js` include regressions where missing cache read caused
large under-counts.

## API Payload Keys

- `model_id` is canonical for pricing, filtering, and aggregation.
- `display_model` is response-only display data.
- Provenance values are limited to the documented `edge`, `cache`, and `mock`
  meanings.

## Aggregate Types

PostgREST aggregate fields are selected as `sum(column)`. Keep parsing helpers
able to handle returned aggregate aliases such as `sum_total_tokens` or explicit
billable fields, as covered by usage aggregate tests.

## Avoid

- Do not use `column.sum()` in InsForge/PostgREST queries.
- Do not calculate dashboard totals from display strings.
- Do not pass raw backend rows directly into UI components without normalization
  at the API/hook boundary.
