# TokenTracker Quality Guidelines

## Local Commands

Use the scripts in `package.json` and `CLAUDE.md`:

- `npm test`
- `npm run ci:local`
- `npm run dashboard:build`
- `npm run validate:copy`
- `npm run validate:ui-hardcode`
- `npm run validate:guardrails`
- `npm --prefix dashboard run build`

Use `npm run dashboard:dev` for the Vite dev server, but verify CLI/backend
changes through `node bin/tracker.js serve --no-sync`.

## Privacy And Token Correctness

- Never collect prompts, messages, or conversation bodies.
- Token channel semantics must stay explicit: input, cached input, cache
  creation input, output, reasoning output, and total.
- When adding a provider, verify raw usage against provider billing behavior
  instead of assuming `input_tokens` semantics.

## Copy And UI Guardrails

- Add user-facing text to `dashboard/src/content/copy.csv`.
- Run copy and hardcoded-string validators for UI text changes.
- Use `focus:ring-inset` for inputs inside height-collapsing motion containers,
  matching the documented Settings page lesson.

## Release Impact

Any `src/` or `dashboard/` change ships both npm and DMG. Do not update only one
surface or version path when behavior is embedded into the native app.

## Avoid

- Do not skip dashboard build after UI or route changes.
- Do not add hardcoded UI strings to pass a visual check quickly.
- Do not accept subagent-reported success without direct file verification.
