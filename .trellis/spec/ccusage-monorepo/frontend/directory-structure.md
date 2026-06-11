# ccusage Directory Structure

## Monorepo Boundaries

Follow the package-local `CLAUDE.md` files before changing a subpackage.

- `apps/ccusage/` is the main bundled CLI.
- `apps/codex/` and `apps/opencode/` are bundled CLI apps with the same runtime
  dependency rule: runtime libraries belong in `devDependencies` so the bundler
  owns the shipped payload.
- `packages/terminal/` owns reusable terminal table formatting utilities.
- `docs/` is the VitePress documentation site and copies generated schema assets
  during its build/dev workflow.

Reference files:

- `ref/ccusage/CLAUDE.md`
- `ref/ccusage/apps/ccusage/CLAUDE.md`
- `ref/ccusage/packages/terminal/CLAUDE.md`
- `ref/ccusage/docs/CLAUDE.md`

## Source Organization

- Command modules live under `apps/ccusage/src/commands/`.
- Shared app internals use underscore-prefixed files such as `_types.ts`,
  `_shared-args.ts`, `_utils.ts`, and `_date-utils.ts`.
- Terminal rendering helpers live in `packages/terminal/src/`.
- Documentation pages and VitePress configuration stay under `docs/`.

## Local Rules

- Keep command modules aligned with CLI reports (`daily`, `monthly`, `session`,
  `blocks`, `statusline`).
- Import local TypeScript modules with `.ts` extensions, matching existing code.
- Export only surfaces consumed by another package or command; bundled CLIs are
  not library-first packages.

## Avoid

- Do not add browser app structure to this spec; ccusage presents via terminal
  output and docs.
- Do not move shared terminal formatting back into individual command modules.
- Do not add runtime dependencies under `dependencies` for bundled CLI apps
  unless the package guidance is intentionally changed.
