# codeburn Type Safety

## TypeScript Strict

The project uses strict TypeScript. `CONTRIBUTING.md` explicitly forbids
unexplained `any`. If an `any` is unavoidable around a third-party or raw
provider payload, add a comment explaining why and narrow it immediately.

## Provider Interfaces

Every provider implements the `Provider` interface in `src/providers/types.ts`:

- discovery returns session sources,
- parsers yield `ParsedProviderCall`,
- display helpers map model and tool names.

Use the existing interface instead of passing provider-specific objects through
parser or UI code.

## Safe Dynamic Data Handling

- Use `Map` or explicit allowlists for parsed input keyed by user/provider data.
- Avoid `obj[key] = value` in `src/providers/` and `src/parser.ts`; Semgrep
  enforces this hot-path rule.
- Use `seenKeys: Set<string>` for dedupe rather than ad hoc object markers.

## Native And Version Boundaries

- `src/cli.ts` uses dynamic `import('./main.js')` so the Node guard remains
  parseable on older Node versions.
- `src/sqlite.ts` wraps `node:sqlite` with helpers such as `blobToText()` and
  `isSqliteBusyError()`; keep native API assumptions there.

## Avoid

- Do not static-import application modules into `src/cli.ts`.
- Do not let Ink views depend on provider-specific payload shapes.
- Do not widen types to make parser tests pass; fix the decoder or fixture.
