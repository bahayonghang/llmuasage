# ccusage Quality Guidelines

## Commands

Use the pnpm commands documented in `ref/ccusage/CLAUDE.md` and package-local
guides:

- `pnpm run format`
- `pnpm typecheck`
- `pnpm run test`
- Package docs also define `pnpm run build`, `pnpm run dev`, and docs-specific
  commands where relevant.

## Formatting And Lint Rules

- ESLint owns formatting.
- Existing guidance specifies tab indentation and double quotes.
- `console.log` is forbidden except where explicitly disabled; use `logger.ts`
  and stdout helpers.
- Local TypeScript imports use `.ts` extensions.

Reference files:

- `ref/ccusage/CLAUDE.md`
- `ref/ccusage/apps/ccusage/CLAUDE.md`
- `ref/ccusage/packages/terminal/CLAUDE.md`

## Test Style

- Vitest tests are commonly colocated in source files inside
  `if (import.meta.vitest != null)` blocks.
- Keep tests deterministic and avoid dynamic imports in test blocks.
- When changing terminal width behavior, test both narrow and wide `COLUMNS`
  cases as `packages/terminal/src/table.ts` does.

## Dependency Rule

Bundled CLI apps put runtime libraries in `devDependencies`. Do not move
libraries to `dependencies` unless package guidance changes explicitly.

## Avoid

- Do not broaden exports just because a helper is convenient in one file.
- Do not add tests that rely on the developer's real Claude data.
- Do not bypass the package-local `CLAUDE.md` for app-specific rules.
