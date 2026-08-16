# ADR 0012 — Antigravity CLI passive parser (unblocking `historical_only`)

- Status: Accepted
- Date: 2026-08-16
- Supersedes: ADR 0011's Antigravity parserless outcome ("Antigravity ... no parser or passive probe"); ADR 0009's "do not register a transcript parser until a real token-bearing Antigravity schema is verified" condition is now satisfied
- Related code: `src/parsers/antigravity.rs`, `src/parsers/source_files.rs`, `src/domain/source_descriptor.rs`, `src/domain/platform_monitor.rs`, `src/store/migrations.rs` (v21), `src/commands/sync.rs`
- Related terms: Source, SourceParser, SourceDescriptor, Platform Monitor, token-accounting marker, FileCursor

## Context

ADR-0011 kept Antigravity as the only persisted source without a parser because
no token-bearing local artifact existed. That premise has reversed: the
Antigravity CLI (a separate product from the IDE) now persists one SQLite
database per conversation under `~/.gemini/antigravity-cli/conversations/`, and
its `gen_metadata` table stores one `GeneratorMetadata` protobuf blob per model
generation with full usage channels.

Evidence (81+ local conversation DBs, 1528→1579 rows, documented in the task
research): usage lives in the nested `chatModel(#1).#4` submessage — the
top-level `#4` field is a constant 36-byte distractor; `#3 == #9 + #10` holds
with zero violations, proving thinking tokens (`#10`) are disjoint from text
output (`#9`); channels cover system prompt (`#1`), non-cached input (`#2`),
cache read (`#5`), output (`#9`), thinking (`#10`), and responseId (`#11`).

## Decision

Flip `antigravity` from `historical_only` to a parser-backed passive source.

- Register `AntigravityParser`: read each `conversations/*.db` read-only,
  decode `gen_metadata` with a hand-written protobuf wire reader (zero new
  dependencies), and map one usage row to one `UsageEvent`. Model attribution
  follows tokscale's `SessionModels` rules: `#21` label → `#19` model mapping,
  ambiguous labels dropped, sole-model fallback, then `antigravity-unknown`.
- Token normalization: `input = #2 + #1`, `cache_read = #5`,
  `output = #9` (text only), `reasoning = #10` (diagnostic channel, but
  included in the total under the disjoint-from-output exception proven by the
  `#3 == #9 + #10` invariant), `total = input + cache_read + output +
  reasoning`. There is no authoritative grand total; the candidate table notes
  the channel-sum total. `cache_creation` stays 0 (no field observed).
- Discovery: `$GEMINI_CLI_HOME/antigravity-cli/conversations` (default
  `~/.gemini/...`) — the same `GEMINI_CLI_HOME` semantics as the gemini
  platform monitor (Gemini root, not the conversations directory).
- Keep the stable `antigravity` SourceKind and id (ADR-0009). The IDE-side
  `~/.gemini/antigravity/conversations/*.pb` family stays monitor-planned:
  no schema exists for it.
- Quality is `precise`; per-file replay uses the shared `FileCursor`
  fingerprint with whole-file reparse and a per-path reset on every rescan
  (Grok session-replay semantics): SQLite files have no stable byte-offset
  contract, so any change reparses the complete conversation and event-key
  idempotency collapses duplicates.

### Hook-era history protection (P0, precondition of the flip)

Hook-era rows predate the parser, carry no `source_path_hash` attribution, and
do not exist in `conversations/*.db`. Three protections land together:

1. **Marker preset migration (v21)** — `preset_antigravity_token_accounting`
   writes `meta['token_accounting_version.antigravity'] = 2` (ON CONFLICT DO
   NOTHING) so both automatic legacy-repair paths (unbounded sync, serve
   startup) treat existing rows as current and never reset the source.
2. **Rebuild guard** — `--rebuild` refuses (absolute; not bypassed by
   `--allow-lossy-rebuild`, covering both targeted and full rebuilds) while
   unattributed antigravity events exist, pointing at export/backup instead.
   Once no unattributed rows remain, rebuild behaves like any parser source.
3. **Generation semantics** — parser-era events are reconstructable from local
   artifacts (normal rebuild semantics); hook-era events are read-only
   protected history that coexists with parser imports (distinct event-key
   shapes: legacy keys like `antigravity:test:event` never collide with
   parser keys `antigravity:<path_hash>::<response_hash>`).

## Consequences

- `llmusage sync` imports Antigravity CLI usage; `source-status` reports
  `passive_ready`/`passive_no_data` with `precise` quality instead of
  `historical_only`.
- Historical dashboard rows remain aggregated and selectable; new parser rows
  add to them without double counting.
- The platform monitor flips to `registered` with the conversations root and
  `*.db` patterns; the previous `blocked_no_samples` wording is retired.
- Upgrades are safe by construction: migration v21 runs before any parser can
  trigger a repair, and the rebuild guard makes destructive paths fail loudly
  instead of silently deleting unreconstructable history.

## Verification

- Wire-decoder unit tests: nested usage decode with top-level `#4` distractor,
  unknown-field skipping, truncated blobs, checksum mismatch issue, all-zero
  skip, SessionModels backfill (label/sole-model/ambiguity), missing
  responseId fallback.
- Integration tests: sync-twice idempotency, append/rewrite replay with stale
  row replacement, deleted-conversation preservation, missing root
  `passive_no_data`, `GEMINI_CLI_HOME` override, hook-era upgrade path
  (marker keeps rows out of automatic repair; unbounded sync preserves them),
  rebuild refusal with unattributed history, bounded-run window filtering
  without cursor advancement or resets.
- Real-data reconciliation: 1573 events across 83 local conversations match an
  independent Python wire decoder exactly on all six channel sums (see task
  research §9).
