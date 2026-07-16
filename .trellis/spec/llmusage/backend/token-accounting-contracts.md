# Token Accounting Contracts

## 1. Scope / Trigger

Read this contract before changing parser token fields, event identity, token
totals, persisted costs, report queries, or token-accounting migration behavior.
The comparable parser-backed sources are Claude, Codex, and OpenCode. ccusage
is the compatibility baseline when reference implementations disagree.

## 2. Signatures

- Parser output: `UsageEvent { tokens: UsageTokens, event_key, ... }`.
- Persisted contract: `usage_event.total_tokens` ->
  `usage_bucket_30m.total_tokens` -> query/UI `total_tokens`.
- Version metadata: `meta('token_accounting_version.<source>') = '2'`.
- Legacy repair: `llmusage sync --rebuild --source <source>`.

## 3. Contracts

- `input_tokens` is non-cached input. Cache read and cache creation/write remain
  separate channels.
- A trustworthy upstream total is parser-owned and authoritative. Fallback
  totals include each input/cache/output channel once.
- Reasoning is diagnostic unless the source contract proves it is disjoint
  from output. Query and UI code must not add it to output or total by default.
- Codex `cached_input_tokens` is inclusive in raw input and must be clamped and
  subtracted. `cache_read_tokens` and `cache_read_input_tokens` are separate
  aliases and do not trigger subtraction.
- Claude dedupes by `message.id + requestId`; sidechain replay can match by
  message id, prefers non-sidechain metadata, and merges streaming channel
  maxima.
- Codex copied events use a source-scoped logical identity derived from
  timestamp, normalized model, and the normalized token tuple.
- OpenCode uses `max(valid tokens.total, input + cache write + cache read + output)`.
- Pricing receives normalized channels. Prompt-tier selection remains
  `input + cache_read + cache_creation`.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Source has rows and marker `2` | Normal incremental writes are allowed |
| Source has rows and no/currently different marker | Refuse normal writes and name the rebuild command |
| Source has no rows and no marker | Allow first sync; write marker only after success |
| Rebuild has missing source files | Existing lossy-rebuild guard refuses it |
| Rebuild parser/store commit fails | Leave marker absent; do not claim parity |
| Parserless source | Do not invent a marker or token normalization |

Never enable `--allow-lossy-rebuild` automatically.

## 5. Good / Base / Bad Cases

- Good: Codex raw input `100`, cached `40`, output `30`, reasoning `10`, total
  `130` persists as input `60`, cache read `40`, output `30`, reasoning `10`,
  total `130`.
- Base: OpenCode without `tokens.total` falls back to known non-reasoning
  components.
- Bad: a report computes `input + cache + output + reasoning` instead of
  summing persisted `total_tokens`.

## 6. Tests Required

- Parser unit tests assert exact integer channel values and total fallbacks.
- `tests/token_accounting_parity.rs` covers all three sources, copied/streaming
  duplicates, event/bucket/query equality, cost tolerance `1e-9`, marker
  advancement, legacy refusal, warning payload, and guarded rebuild.
- `tests/sync_regression.rs` keeps hot sync, append, replacement, and rebuild
  behavior idempotent.
- Run `cargo test -- --test-threads=1` and `just ci` for cross-layer changes.

## 7. Wrong vs Correct

### Wrong

```sql
SUM(input_tokens) + SUM(cache_read_tokens) + SUM(output_tokens) +
SUM(reasoning_output_tokens)
```

### Correct

```sql
SUM(total_tokens)
```

The corrected query preserves source semantics and prevents visible diagnostic
subchannels from being charged or displayed twice.
