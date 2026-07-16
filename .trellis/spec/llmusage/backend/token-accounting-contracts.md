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
- Serve startup repair:
  `commands::serve::repair_legacy_token_accounting(&AppContext, &Store) -> Result<TokenAccountingRepairReport>`.
- Repair reports list `rebuilt_sources` and `blocked_sources`; each blocked row
  includes `source`, `missing_file_count`, and `protected_event_count`.

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
- `llmusage serve` detects legacy parser sources after store bootstrap and
  before binding a port. It rebuilds safe sources one at a time in parser
  registry order with `allow_lossy_rebuild=false`.
- A known lossy legacy source is reported as blocked without deleting history;
  the dashboard may start, but normal writes remain guarded for that source.
- Parser, SQLite, commit, or risk-query errors for an otherwise automatic
  repair propagate and stop dashboard startup.
- A no-source full rebuild derives its preflight, reset, marker-clear, and
  parser fan-out boundaries from the same parser collection. It calls
  `Store::reset_for_source` for each parser source and preserves parserless
  events, buckets, behavior facts, cursors, and source-file state.
- `Store::reset_usage_data` is a low-level global reset surface. Command-level
  full rebuild must not call it because it has no parser capability boundary.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Source has rows and marker `2` | Normal incremental writes are allowed |
| Source has rows and no/currently different marker | Refuse normal writes and name the rebuild command |
| Source has no rows and no marker | Allow first sync; write marker only after success |
| Rebuild has missing source files | Existing lossy-rebuild guard refuses it |
| Rebuild parser/store commit fails | Leave marker absent; do not claim parity |
| Parserless source | Do not invent a marker or token normalization |
| Serve finds safe legacy parser source | Rebuild before binding the dashboard port |
| Serve finds lossy legacy parser source | Warn, preserve history and marker state, continue startup |
| Serve repair risk query or safe rebuild fails | Return the error and do not bind the port |
| Full rebuild includes parserless history | Preserve it; reset only parser registry sources |

Never enable `--allow-lossy-rebuild` automatically.

## 5. Good / Base / Bad Cases

- Good: Codex raw input `100`, cached `40`, output `30`, reasoning `10`, total
  `130` persists as input `60`, cache read `40`, output `30`, reasoning `10`,
  total `130`.
- Base: OpenCode without `tokens.total` falls back to known non-reasoning
  components.
- Bad: a report computes `input + cache + output + reasoning` instead of
  summing persisted `total_tokens`.
- Good: serve repairs Codex, Claude, and OpenCode in registry order while an
  unrelated parserless Antigravity archive remains untouched.
- Base: an already-current or empty parser source makes serve repair a no-op.
- Bad: a full rebuild calls `reset_usage_data`, deleting parserless history
  that no registered parser can reconstruct.

## 6. Tests Required

- Parser unit tests assert exact integer channel values and total fallbacks.
- `tests/token_accounting_parity.rs` covers all three sources, copied/streaming
  duplicates, event/bucket/query equality, cost tolerance `1e-9`, marker
  advancement, legacy refusal, warning payload, and guarded rebuild.
- `tests/sync_regression.rs` keeps hot sync, append, replacement, and rebuild
  behavior idempotent.
- Serve repair tests assert safe marker advancement, normal-sync unblocking,
  registry order, lossy blocked counts, preserved history, and propagated
  parser failure.
- Full rebuild tests seed parserless event, bucket, behavior, cursor, and
  source-file rows, then assert every row survives the rebuild.
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

For full rebuild deletion boundaries:

### Wrong

```rust
store.reset_usage_data()?;
```

### Correct

```rust
for source in parser_sources {
    store.reset_for_source(source)?;
}
```

The correct form cannot delete a parserless source that the subsequent parser
fan-out is unable to reconstruct.
