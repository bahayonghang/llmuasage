# ADR 0009 — Antigravity source cutover

- Status: Accepted
- Date: 2026-05-28
- Supersedes: ADR 0008's Antigravity/Gemini alias assumption
- Related code: `src/domain/models.rs`, `src/domain/source_descriptor.rs`, `src/integrations/antigravity.rs`, `src/store/migrations.rs`
- Related terms: Source, SourceParser, Integration, HookTarget, Store

## Context

The previous compatibility design kept `gemini` as the public and persisted source id while allowing Antigravity-facing copy and hook paths. That preserved old CLI filters and the legacy Gemini transcript parser, but it made the Google local CLI source ambiguous: the source id looked like a tool that is no longer the current hook target.

Antigravity still uses a `~/.gemini/config/hooks.json` path today. That filesystem path is not a source id. Model names such as `gemini-2.5-pro` are also model ids, not source ids.

## Decision

Make `antigravity` the only public Google local CLI source id.

- `SourceKind::Antigravity::as_str()` returns `antigravity`.
- `gemini` is not accepted by source-id parsing, CLI filters, hook-run input, or dashboard/API filters.
- Antigravity remains integration-only: llmusage installs `~/.gemini/config/hooks.json::Stop` and does not register a transcript parser until a real token-bearing Antigravity schema is verified.
- Legacy llmusage-owned `--source gemini` hook commands are cleaned up best-effort during init/uninstall from both Antigravity `hooks.json::Stop` and old Gemini `settings.json::hooks.SessionEnd` without deleting user commands.
- Schema migration v13 rewrites historical source values from `gemini` to `antigravity` in source-bearing tables and moves `source.gemini.enabled` metadata to `source.antigravity.enabled`.
- Event keys and raw join keys are not rewritten; they are idempotency keys, not the source-id column.

## Consequences

Historical reports continue under `antigravity` after migration. Users who pass `--source gemini` get the normal clap invalid-value error and should use `--source antigravity`.

The parser registry can be smaller than the source descriptor/integration registry. Descriptor invariants therefore check that parser sources are covered, while integration-only sources are allowed to have `capabilities.parser = false`.

## Rejected alternatives

- Keep `gemini` as an input alias: rejected because the cutover requires one public source id and explicit failure for stale filters.
- Rewrite model names containing `gemini-*`: rejected because model ids are provider/model names, not source ids.
- Guess an Antigravity transcript parser from old Gemini files: rejected because no verified Antigravity token schema is available.
- Delete historical `gemini` rows: rejected because migration can preserve statistical continuity by updating source columns.

## Verification

- Source parsing tests assert `antigravity` parses and `gemini` does not.
- Migration tests assert source-bearing tables move `gemini` rows to `antigravity`, metadata is renamed, and event keys remain unchanged.
- Integration tests assert Antigravity installs only a `Stop` hook with `--source antigravity`, removes llmusage-owned legacy `--source gemini` hooks, and preserves user hooks.
- Docs and CLI help list `antigravity` rather than `gemini` as the Google local CLI source id.
