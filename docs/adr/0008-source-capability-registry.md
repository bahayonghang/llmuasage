# ADR 0008 — Source Capability Registry and passive-reader gate

- Status: Accepted, partly superseded by [ADR 0009](./0009-antigravity-source-cutover) for Antigravity/Gemini source-id aliasing
- Date: 2026-05-28
- Related code: `src/domain/source_descriptor.rs`, `src/registry.rs`, `src/commands/source_status.rs`, `src/commands/hook_run.rs`
- Related terms: Source, SourceParser, Integration, Registry, HookTarget

## Context

`SourceKind` is the stable persisted source id, while `registered_parsers()` and `registered_integrations()` are the executable fan-out points. That shape works for the current four sources, but it does not describe activation mode, aliases, status semantics, token quality, or passive-reader safety.

Without a capability layer, future sources would push metadata into status text, init/probe code, parser onboarding notes, and hook-run filtering independently.

## Decision

Add `SourceDescriptor` as static domain metadata for each supported source.

Each descriptor declares:

- stable id and aliases,
- display name,
- activation mode (`hook`, `plugin`, `passive`, or `hybrid`),
- parser/integration/hook/passive capabilities,
- token quality,
- local privacy boundary.

`SourceKind::as_str()` remains the storage id. Descriptors may declare aliases for future non-breaking inputs, but Antigravity/Gemini is no longer such an alias pair: ADR 0009 makes `antigravity` the persisted source id and removes `gemini` as a source id.

The registry now exposes descriptor accessors and invariant tests assert that descriptors, parser registration, and integration registration stay aligned.

## Hook-run consequence

Hook-triggered sync is source-aware. `hook-run --source <source>` still records `trigger_state`, uses the worker lock, recovers stale runs, and performs snapshot catch-up, but the worker sync is filtered to the triggering source.

Codex keeps a singleton notify integration. If install backed up a distinct original notify, hook runtime starts that original notify after llmusage handling. Chaining is best-effort, skips llmusage/self commands, and does not block the hook result.

## Passive-reader gate

A descriptor can declare passive capability, but parser implementation is still gated by evidence:

1. real local samples,
2. fixture tests,
3. sync-twice idempotency,
4. cursor/rotation/truncation or DB rebuild behavior,
5. token-quality declaration,
6. privacy review.

No passive parser should be added from README claims or reference-code guesses alone.

## Rejected alternatives

- Keep all metadata on `SourceKind`: rejected because enum matches would grow across status, hook, parser, docs, and UI code.
- Treat every source as an `Integration`: rejected because passive readers do not write external configuration.
- Run every parser on every hook signal: rejected because hook signals should be low-blocking and source-aware.

## Verification

- Registry invariant tests cover descriptor/parser/integration drift.
- Source status tests cover configured vs degraded hook-missing states.
- Hook-run regression test verifies a Claude hook imports only Claude data when other fixtures exist.
