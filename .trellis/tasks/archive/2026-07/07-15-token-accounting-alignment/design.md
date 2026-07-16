# Design: ccusage-Compatible Token Accounting

## Status

Planning ready for final review. Token semantics and legacy-data behavior are
approved; implementation still requires the Trellis `task.py start` gate.

## Design Goals

1. Match ccusage exactly for comparable Claude, Codex, and OpenCode token fields.
2. Keep llmusage's parser -> SQLite -> query architecture and offline pricing catalog.
3. Make token total source-aware instead of reconstructing it from every visible field.
4. Correct existing databases without silently mixing legacy and corrected accounting.
5. Preserve incremental sync where it does not weaken deduplication correctness.

## Non-Goals

- Adding new parser-backed platforms.
- Adopting ccusage/tokscale as runtime dependencies.
- Matching ccusage's network pricing lifecycle.
- Redesigning CLI, TUI, or Web presentation beyond corrected values and warnings.

## Canonical Contract

`UsageTokens` keeps its current public fields, but their ownership changes:

```text
input_tokens             = non-cached input only
cache_creation_tokens    = cache write/create input
cache_read_tokens        = cached input reused by the request
output_tokens            = provider-reported output channel
reasoning_output_tokens  = diagnostic subchannel
total_tokens             = parser-owned normalized total
```

Rules:

- Consumers sum persisted `total_tokens`; they do not derive it from components.
- Cache channels may contribute to total exactly once.
- Reasoning contributes only when a source contract proves it is disjoint from
  output. Visibility does not imply additivity.
- Pricing consumes normalized input/cache/output channels. The existing
  `ReasoningPolicy` remains the authority for separate reasoning billing.
- Malformed negative values clamp to zero at the parser boundary.

## Source Normalization

### Claude

```text
input  = usage.input_tokens
write  = aggregate cache_creation_input_tokens fields
read   = usage.cache_read_input_tokens
output = usage.output_tokens
total  = input + write + read + output
reasoning = parsed for diagnostics only when present; not additive by default
```

Deduplication identity:

- Primary: `message.id + requestId`.
- Fallback: `message.id` when request id is absent.
- Sidechain replay: prefer a non-sidechain parent entry.
- Streaming repeat: keep per-channel maxima and the latest complete metadata;
  never sum repeated snapshots.
- Missing identity: retain the current file/fingerprint/offset fallback.

Changed Claude JSONL files must be parsed from the beginning so a streaming
repeat appended after the previous cursor can replace, rather than add to, the
earlier snapshot. Unchanged files remain skippable.

### Codex

For standard Codex `token_count` records:

```text
raw_input = input_tokens
read      = min(cached_input_tokens, raw_input)
input     = raw_input - read
output    = output_tokens
reasoning = reasoning_output_tokens
total     = trustworthy upstream total_tokens
            or input + read + output when total is absent
```

`cache_read_tokens` and nested OpenAI response-detail fields must be classified
by their documented semantics. Aliases must not all share one subtraction rule.

Continue using `last_token_usage` as the primary increment. Use cumulative
`total_token_usage` only for baseline/delta/fork checks. Port stale-regression
and fork-replay cases from tokscale where they improve the current cursor logic.

Copied/archive/fork event identity follows ccusage's stable tuple:

```text
timestamp + normalized model + input + cache_read + output + reasoning + total
```

The persistent key is source-scoped but not file-scoped, so the same logical
event copied to another session artifact is ignored by SQLite dedupe.

### OpenCode

```text
input     = tokens.input
write     = tokens.cache.write
read      = tokens.cache.read
output    = tokens.output
reasoning = tokens.reasoning
known     = input + write + read + output
total     = max(valid tokens.total, known)
```

This matches ccusage's authoritative-total fallback. Reasoning remains visible,
but is not added again when upstream total already contains it. If `tokens.total`
is absent, the ccusage-compatible fallback is `known`; tokscale's larger
reasoning-inclusive total is recorded as a deliberate reference difference.

OpenCode continues to use stable message ids/high-water pagination. Add fixture
coverage for replacement DBs, equal timestamps, and `tokens.total` gaps.

## Deduplication And Incremental Sync

Stable logical event keys are required but not sufficient because reset is
currently owned by `source_path_hash`.

The implementation must enforce these reconciliation rules:

1. Normal append with an already-seen logical key does not add a bucket row.
2. A more complete Claude streaming snapshot replaces the existing logical
   event and adjusts its old/new bucket and cost atomically.
3. Rewriting or deleting the file that currently owns a cross-file duplicate
   cannot make the logical event disappear while another live copy exists.
4. Any replay that can change dedupe ownership performs a source reconciliation
   pass before cursor commit.

The minimal store extension is an event upsert/reconcile path, not a second
usage aggregation system. It should:

- load the existing event for a stable key;
- apply source-specific winner rules;
- subtract the previous event from its bucket when replacement wins;
- insert/add the replacement event and cost;
- keep event, bucket, source-file state, and cursor changes in one shard
  transaction.

If benchmarks show that full changed-file replay is too expensive, a later
optimization may persist parser-local dedupe state. That optimization is not
required for correctness in this task.

## Query And Display Flow

```text
parser-owned total
  -> usage_event.total_tokens
  -> usage_bucket_30m.total_tokens
  -> report/query payload total_tokens
  -> CLI / JSON / TUI / Web
```

Required query rule:

- Event paths use `SUM(usage_event.total_tokens)`.
- Bucket paths use `SUM(usage_bucket_30m.total_tokens)`.
- Derived shares use stored total as denominator.
- `output_tokens` is not combined with reasoning unless the UI explicitly
  labels the result as an expanded diagnostic value.

JSON keeps existing snake_case field names. No field removal is required.

## Cost Flow

- Parser normalization happens before pricing.
- Corrected Codex non-cached input prevents cache reads from being charged once
  at input rate and again at cache-read rate.
- Request-tier selection remains
  `input + cache_read + cache_creation` per the pricing catalog contract.
- Buckets/reports continue summing persisted event costs.
- Reference parity tests compare costs only under the same fixed rate table;
  ccusage's live models.dev/LiteLLM data is not a stable CI oracle.

## Token Semantics Version And Legacy Data

SQL cannot reliably repair current rows because old events do not record whether
their input alias was inclusive and cannot reconstruct source-level duplicates.

Introduce a per-source token-accounting marker in existing `meta` storage:

```text
token_accounting_version.codex = 2
token_accounting_version.claude = 2
token_accounting_version.opencode = 2
```

Behavior:

- Fresh databases write version 2 after a successful first source sync.
- Existing sources with rows and no version marker are legacy.
- A successful source rebuild writes the marker only after parser/store commit.
- Reports and `source-status` expose a legacy-accounting warning until the
  source is rebuilt.
- Normal sync must not silently claim parity while legacy rows remain.

Approved migration policy:

- Require explicit `llmusage sync --rebuild --source <source>` for legacy rows.
- Reuse the existing lossy-rebuild guard. If source files are missing, preserve
  legacy data and report that parity is blocked; do not auto-delete it.
- Do not set the version marker when parsing or commit fails.
- Refuse normal writes for a stale source until its guarded rebuild succeeds.
  Keep existing reports read-only and show an explicit legacy-accounting warning.

## Compatibility

- No source id changes.
- No token column rename or removal.
- Existing JSON field names stay stable.
- Correct totals and costs are intentional behavior changes.
- TUI/Web routes keep their payload shapes.
- Antigravity stays integration-only and is not assigned invented token data.

## Rollout And Rollback

Rollout:

1. Land contract tests and parser/store/query changes together.
2. Mark legacy sources and document guarded rebuild commands.
3. Rebuild one source at a time, compare fixtures and local summaries to
   ccusage, then rebuild all sources if desired.

Rollback:

- Code rollback can continue reading the unchanged token columns.
- Keep a database backup before the first production rebuild.
- If a rebuild reveals missing source artifacts, stop and retain the pre-rebuild
  database; do not use `--allow-lossy-rebuild` without explicit approval.

## Key Trade-offs

| Decision | Benefit | Cost |
| --- | --- | --- |
| Persist parser-owned total | Matches source semantics and ccusage | Requires auditing every query formula |
| Stable logical event keys | Cross-file dedupe | Requires replacement/ownership reconciliation |
| Full replay of changed Claude files | Correct streaming max merge | More JSONL read work on changed files |
| Explicit guarded rebuild | No silent destructive migration | User must take a one-time action |
| ccusage over tokscale on disagreement | Clear acceptance oracle | Some tokscale reasoning totals remain intentionally different |
