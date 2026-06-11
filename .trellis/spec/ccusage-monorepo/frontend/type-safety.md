# ccusage Type Safety

## TypeScript ESM

The monorepo uses TypeScript ESM. Match existing imports:

- Use `.ts` extensions for local imports.
- Keep CLI app entrypoints bundled and executable-focused.
- Export only the types/functions consumed across modules or packages.

## Schemas And Branded Types

`apps/ccusage/src/_types.ts` is the reference for Valibot schemas and branded
types. Use its helper functions when converting raw strings into typed values.

Other schema examples:

- `commands/statusline.ts` validates hook JSON with Valibot.
- `_shared-args.ts` defines shared CLI option schemas.

## Error Handling

Prefer `@praha/byethrow` `Result` for operations that can fail:

- Use `Result.try()` to wrap throwing operations.
- Use `Result.isFailure()` and early returns for clear control flow.
- Use `Result.unwrap(defaultValue)` only when the default is an intentional
  local fallback, as in `_utils.ts`.

## Avoid

- Do not pass unvalidated hook/config JSON into report logic.
- Do not introduce unexplained `any`.
- Do not replace existing Result pipelines with broad try/catch blocks unless
  the surrounding module already uses try/catch for that specific hot path.
