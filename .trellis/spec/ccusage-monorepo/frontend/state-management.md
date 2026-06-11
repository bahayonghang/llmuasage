# ccusage State Management

ccusage state is command-local and derived. There is no frontend store.

## Command State

Command modules derive their state from:

- merged CLI/config options,
- parsed usage records,
- selected output mode (`--json` versus table),
- terminal width for compact mode.

Examples:

- `apps/ccusage/src/commands/monthly.ts` computes `useJson`, silences logging
  when needed, builds a JSON object or a `UsageReportConfig`, then writes output.
- `packages/terminal/src/table.ts` stores table rows and compact-mode status
  inside `ResponsiveTable`.

## Config State

Configuration discovery and merge diagnostics belong in the config loader
helpers, not in table rendering. Tests in `_config-loader-tokens.ts` assert
messages such as selected config path, defaults, command configs, and final
merged options.

## Local Rules

- Keep report totals and model breakdowns as derived data passed into renderers.
- Keep terminal compact mode inside `ResponsiveTable`; callers should read
  `table.isCompactMode()` only for user hints.
- Keep logger state changes near the command output boundary.

## Avoid

- Do not add global mutable state for report data.
- Do not let terminal table code read config files.
- Do not let JSON mode inherit table-only hints or warnings.
