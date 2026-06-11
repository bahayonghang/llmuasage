# codeburn State Management

## TUI State

Interactive views keep state local:

- selected period,
- provider filter,
- active view/tab,
- loading/error status,
- reload counters,
- measured terminal width.

Do not introduce a global UI store for these terminal views unless multiple
independent views need the same mutable state.

## Parser State

`src/parser.ts` owns aggregation state:

- `parseAllSessions()` is the central entry point.
- `seenKeys` deduplicates turns that appear through multiple providers.
- Maps and Sets track projects, sessions, models, tools, and provider grouping.
- Public exports include `parseAllSessions`, `filterProjectsByName`, and
  `extractMcpInventory`.

Reference files:

- `ref/codeburn/docs/architecture.md`
- `ref/codeburn/src/parser.ts`
- `ref/codeburn/tests/providers/opencode.test.ts`
- `ref/codeburn/tests/providers/pi.test.ts`

## Provider Registry State

`src/providers/index.ts` is the single registration point for providers. Keep
eager and lazy provider lists flowing into the same `getAllProviders()` path.
Silent lazy import failure currently excludes that provider from a run; changing
that behavior needs tests and user-facing error design.

## Avoid

- Do not duplicate dedupe state inside individual report renderers.
- Do not use object bracket assignment for parsed user/provider input in parser
  hot paths; use `Map` or explicit allowlists.
- Do not let provider parsers read global UI state.
