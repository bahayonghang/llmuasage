# codeburn Directory Structure

## Runtime Surfaces

`docs/architecture.md` describes CodeBurn as one Node.js CLI plus GUI clients
that shell out to it.

- `src/cli.ts` is the Node-version guard and launcher.
- `src/main.ts` and command files own command dispatch.
- `src/dashboard.tsx` and `src/compare.tsx` own Ink interactive terminal views.
- `src/parser.ts` is the central aggregator.
- `src/providers/` contains one provider integration per AI tool plus shared
  provider types and helpers.
- `src/menubar-json.ts` defines the payload consumed by macOS and GNOME clients.
- `src/sqlite.ts` wraps read-only `node:sqlite` usage.
- `tests/` contains Vitest specs; `tests/providers/` covers provider parsers.

## Client Boundaries

The macOS menubar and GNOME extension do not share code with the CLI. They call:

`codeburn status --format menubar-json --period <p>`

Keep the JSON payload contract in sync between `src/menubar-json.ts`,
`tests/menubar-json.test.ts`, and the client mirrors documented in
`docs/architecture.md`.

## Provider Boundaries

New providers go through `src/providers/index.ts`. Core providers are eager;
providers with heavy native dependencies should be lazy-loaded through the
registry instead of slowing down all users.

## Avoid

- Do not put provider-specific parsing inside TUI components.
- Do not bypass `src/parser.ts` for aggregate report state.
- Do not change the menubar JSON shape without updating its tests and client
  mirrors.
