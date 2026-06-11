# codeburn Frontend Guidelines

`ref/codeburn` is a Node.js TypeScript ESM CLI with terminal output, Ink/React
interactive views, and GUI clients that consume the CLI's `menubar-json`
contract. Treat this frontend layer as CLI presentation plus TUI/client output
contracts.

## Pre-Development Checklist

- Read [Directory Structure](./directory-structure.md) before moving CLI, provider, TUI, or client-contract files.
- Read [Component Guidelines](./component-guidelines.md) before changing Ink views, terminal output, or `menubar-json`.
- Read [Hook Guidelines](./hook-guidelines.md) before editing Ink hooks, async effects, or lazy native dependencies.
- Read [State Management](./state-management.md) before changing parser aggregation, dedupe, provider registration, or TUI state.
- Read [Quality Guidelines](./quality-guidelines.md) before changing providers, tests, build scripts, or PR-facing contracts.
- Read [Type Safety](./type-safety.md) before changing provider types, parser maps, SQLite wrappers, or Node guards.
- Also read `.trellis/spec/guides/index.md` for shared cross-layer and reuse checks.

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Directory Structure](./directory-structure.md) | CLI, parser, providers, TUI, client contracts | Documented |
| [Component Guidelines](./component-guidelines.md) | Ink UI and menubar JSON presentation rules | Documented |
| [Hook Guidelines](./hook-guidelines.md) | Ink hooks, effects, refs, and native lazy loading | Documented |
| [State Management](./state-management.md) | TUI local state, parser Maps/Sets, provider registry | Documented |
| [Quality Guidelines](./quality-guidelines.md) | npm scripts, provider fixtures, Semgrep, contract tests | Documented |
| [Type Safety](./type-safety.md) | Strict TS, provider interfaces, Map/allowlist safety | Documented |

## Quality Check

- For spec-only changes, scan `.trellis/spec/codeburn/frontend/` for template
  markers and trailing whitespace.
- For runtime changes, run the relevant npm command from
  `quality-guidelines.md`, usually `npm test` or a targeted provider suite.
- For `menubar-json` or provider changes, verify the matching contract or
  fixture tests are updated.

## Core References

- `ref/codeburn/CONTRIBUTING.md`
- `ref/codeburn/docs/architecture.md`
- `ref/codeburn/src/cli.ts`
- `ref/codeburn/src/dashboard.tsx`
- `ref/codeburn/src/parser.ts`
- `ref/codeburn/src/providers/index.ts`
- `ref/codeburn/src/menubar-json.ts`
- `ref/codeburn/src/sqlite.ts`
- `ref/codeburn/tests/menubar-json.test.ts`

All codeburn spec documentation is written in English.
