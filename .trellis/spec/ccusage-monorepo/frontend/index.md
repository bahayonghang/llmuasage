# ccusage Monorepo Frontend Guidelines

In `ref/ccusage`, the relevant frontend surface is terminal presentation,
stable JSON output, and VitePress documentation. The CLI apps are bundled
TypeScript ESM executables; there is no React dashboard layer in this package.

## Pre-Development Checklist

- Read [Directory Structure](./directory-structure.md) before changing apps, packages, or docs paths.
- Read [Component Guidelines](./component-guidelines.md) before changing terminal tables or command output.
- Read [Hook Guidelines](./hook-guidelines.md) before changing statusline/hook input handling or data-loading helpers.
- Read [State Management](./state-management.md) before changing command options, config merging, or compact-mode state.
- Read [Quality Guidelines](./quality-guidelines.md) before editing tests, dependencies, formatting, or docs.
- Read [Type Safety](./type-safety.md) before changing schemas, branded types, imports, or error handling.
- Also read `.trellis/spec/guides/index.md` for shared cross-layer and reuse checks.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | Monorepo apps, terminal package, docs ownership | Documented |
| [Component Guidelines](./component-guidelines.md) | Terminal table and command-output composition | Documented |
| [Hook Guidelines](./hook-guidelines.md) | CLI hook/statusline and helper boundaries | Documented |
| [State Management](./state-management.md) | Command-local state, config merge, compact mode | Documented |
| [Quality Guidelines](./quality-guidelines.md) | pnpm, ESLint, in-source Vitest, dependency rules | Documented |
| [Type Safety](./type-safety.md) | TypeScript ESM, Valibot, Result, import rules | Documented |

## Quality Check

- For spec-only changes, scan `.trellis/spec/ccusage-monorepo/frontend/` for
  template markers and trailing whitespace.
- For package changes, use the local pnpm checks from `quality-guidelines.md`:
  `pnpm run format`, `pnpm typecheck`, and `pnpm run test`.
- For terminal output changes, include a focused check around
  `packages/terminal/src/table.ts` or the touched command module.

## Core References

- `ref/ccusage/CLAUDE.md`
- `ref/ccusage/apps/ccusage/CLAUDE.md`
- `ref/ccusage/apps/codex/CLAUDE.md`
- `ref/ccusage/apps/opencode/CLAUDE.md`
- `ref/ccusage/packages/terminal/CLAUDE.md`
- `ref/ccusage/docs/CLAUDE.md`
- `ref/ccusage/apps/ccusage/src/commands/*.ts`
- `ref/ccusage/packages/terminal/src/table.ts`

All ccusage spec documentation is written in English.
