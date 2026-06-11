# codeburn Quality Guidelines

## Local Commands

Use commands from `CONTRIBUTING.md`:

- `npm run dev -- status`
- `npm test`
- `npm test -- tests/providers/codex.test.ts` for a targeted suite
- `npm run build`
- `npm run bundle-litellm` when intentionally refreshing the pricing snapshot

The project expects Node.js 22.x for development. `src/cli.ts` must remain
parseable by Node 18 because it performs the version guard before dynamic
importing `main.js`.

## Provider Quality Bar

Provider parsers must be deterministic and fixture-backed:

- Add or update `tests/providers/<provider>.test.ts` for parser changes.
- New providers go through `src/providers/index.ts`.
- Lazy-load providers that pull heavy native dependencies.
- Include real-session evidence for new providers as requested in
  `CONTRIBUTING.md`.

## Contract Tests

- If `menubar-json` changes, update `tests/menubar-json.test.ts`.
- If optimize detectors change, include positive and negative cases in
  `tests/optimize.test.ts`.
- If SQLite helpers change, update focused tests such as
  `tests/blob-to-text.test.ts`.

## Static Safety

CI runs a Semgrep guard against bracket assignment in `src/providers/` and
`src/parser.ts`. Keep hot paths on `Map` or allowlists.

## Avoid

- Do not introduce unexplained `any`; TypeScript strict mode is on.
- Do not rely on online docs alone for a new provider parser.
- Do not add a provider without local or fixture evidence of nonzero costs,
  model resolution, and session counts.
