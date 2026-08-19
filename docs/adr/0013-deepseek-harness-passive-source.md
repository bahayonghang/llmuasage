# ADR 0013 — DeepSeek Harness passive source and zstd dependency

- Status: Accepted
- Date: 2026-08-16
- Related code: `src/parsers/dsh.rs`, `src/parsers/source_files.rs`, `src/domain/source_descriptor.rs`, `src/domain/platform_monitor.rs`, `Cargo.toml`
- Related terms: Source, SourceParser, SourceDescriptor, Platform Monitor, FileCursor, session family

## Context

DeepSeek Harness (`dsh`) writes local session logs under `~/.dsh/sessions/` as
multi-frame zstd JSONL (`session.jsonl.zstd`) or uncompressed `session.jsonl`
when `compression: none`. Local samples (15 sessions) and the official
TokenUsage contract are sufficient for a passive parser. The old
`~/.deepseek` root still exists on some machines and must stay monitor-only.

A streaming decoder is required: each flush appends one zstd frame, and a
scan that hits an active file can see a torn tail frame. One-shot
`decode_all` would drop the whole session.

## Decision

Register `SourceKind::DeepseekHarness` (`deepseek_harness`) with a parser in
the same change. Do not introduce a parserless `SourceKind` intermediate
state.

- Discover `$DSH_HOME` (default `~/.dsh`) / `sessions/` at any depth. Match
  only files named `session.jsonl` or `session.jsonl.zstd`.
- Dispatch compression by the zstd frame magic `0x28 B5 2F FD`, not the
  extension.
- Decode with the `zstd` crate streaming `Decoder`. On a mid-stream error,
  keep the already-decoded prefix. Split lines with the 4 MiB record limit
  and partial-tail contract used by other JSONL parsers.
- Import only `assistant/message` usage. Skip `assistant/chunk` duplicates,
  all-zero usage, non-positive `time`, and `seq < seedLength`.
- Event keys always include message identity (or `sid:` fallback), time,
  provider, model, and every token channel so redacted placeholder ids stay
  distinct and fork copies collapse.
- On an unbounded fingerprint change, replay the session family (parent plus
  `parentSession` children) so a rewritten owner cannot drop a shared key
  still present in an unchanged copy.
- Normalize: `input = inputTokens` (already non-cached), `cache_read =
  cacheReadTokens`, `cache_creation = cacheWriteTokens`, `output =
  outputTokens` (includes reasoning), `reasoning` diagnostic only, `total =
  input + cache_read + cache_creation + output`.
- First release is `Unpriced`. The monitor also probes `~/.deepseek`.

The `zstd` crate is the accepted decoder. It matches the tokscale reference
and already builds on the project's Windows CI. `ruzstd` remains the fallback
only if the C binding is later rejected by the CI toolchain contract.

## Consequences

- `Cargo.toml` gains a C-binding dependency. MSRV remains the declared
  `rust-version` and must stay proven with an isolated `CARGO_TARGET_DIR`.
- `source-status` reports `passive_ready` / `passive_no_data` for
  `deepseek_harness`.
- Session logs contain prompts and tool results. The parser reads only usage,
  model/provider scalars, timestamps, and hashed cwd.

## Verification

- Unit tests cover magic-byte dispatch, torn-frame prefix recovery, oversized
  records, seedLength, placeholder ids, and official/local token numbers.
- `tests/sync_regression.rs` covers sync-twice, append, rewrite, delete,
  missing root, `DSH_HOME`, fork folding, family replay, and bounded runs.
