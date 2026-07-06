# codeburn Component Guidelines

## Ink Components

Interactive UI is implemented with Ink/React in files such as
`src/dashboard.tsx` and `src/compare.tsx`.

Local patterns:

- Keep small component functions near the view they serve when they are not
  reused elsewhere.
- Use terminal width to select layout density rather than assuming a fixed
  viewport.
- Use `Box`, `Text`, `useInput`, `useApp`, and `useStdout` from Ink for
  terminal rendering and interaction.
- Guard interactive views when stdout is not an interactive terminal; for
  example `renderCompare()` writes a plain message and returns.

Reference files:

- `ref/codeburn/src/dashboard.tsx`
- `ref/codeburn/src/compare.tsx`

## Structured Output Components

`menubar-json` is a component-level contract for external clients. Keep it
small, deterministic, and pinned by tests.

Reference files:

- `ref/codeburn/src/menubar-json.ts`
- `ref/codeburn/tests/menubar-json.test.ts`
- `ref/codeburn/tests/cli-status-menubar.test.ts`

## Avoid

- Do not make TUI components read raw provider files directly.
- Do not add animation or layout state that cannot work in a terminal.
- Do not treat `menubar-json` as an internal-only shape; GUI clients parse it.
