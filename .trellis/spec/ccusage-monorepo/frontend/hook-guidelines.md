# ccusage Hook Guidelines

ccusage has no React hook layer. Treat "hooks" in this package as CLI hook
payload handling, statusline input, and command helper boundaries.

## Statusline And Hook Input

`apps/ccusage/src/commands/statusline.ts` is the reference for validating hook
JSON:

- It uses Valibot schemas for incoming hook data.
- It silences the logger for statusline output.
- It uses `@praha/byethrow` `Result` pipelines around operations that can fail.

When adding hook-like CLI inputs, validate at the command boundary and convert
raw JSON into typed values before using it downstream.

## Data Loading Helpers

Keep reusable command behavior in helper modules rather than copying logic
across reports:

- `_config-loader-tokens.ts` owns config discovery, merge diagnostics, and
  command option merging.
- `_date-utils.ts`, `_project-names.ts`, and `_token-utils.ts` own repeated
  formatting or normalization behavior.
- `_types.ts` owns branded schemas and parsing helpers.

## Tests

Use in-source Vitest blocks where the package already does:

```ts
if (import.meta.vitest != null) {
  const { describe, it, expect } = import.meta.vitest;
}
```

Reference files:

- `ref/ccusage/apps/ccusage/src/commands/statusline.ts`
- `ref/ccusage/apps/ccusage/src/_utils.ts`
- `ref/ccusage/apps/ccusage/src/_project-names.ts`

## Avoid

- Do not introduce React-specific guidance or `use*` hooks in this package.
- Do not parse hook payload fields with repeated local casts in multiple commands.
- Do not use dynamic imports inside in-source tests; existing guidance requires
  static access through `import.meta.vitest`.
