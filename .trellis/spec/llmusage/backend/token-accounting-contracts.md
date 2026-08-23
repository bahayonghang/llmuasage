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
- Version metadata:
  `meta('token_accounting_version.codex') = '3'`;
  `meta('token_accounting_version.grok') = '3'`;
  `meta('token_accounting_version.pi') = '3'`; Claude, OpenCode, Antigravity,
  Kimi Code, Oh My Pi (`omp`), ZCode, and DeepSeek Harness remain `2`.
  `expected_token_accounting_version(SourceKind) -> u32` owns this source-aware
  contract.
- Legacy repair: `llmusage sync --rebuild --source <source>`.
- Normal-sync repair lifecycle:
  `SyncEvent::TokenAccountingRepairStarted/TokenAccountingRepairFinished`.
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
- OpenAI/Codex usage has no independent cache-write channel. Persist
  `cached_input_tokens` as cache read; `cache_creation_tokens = 0` is correct
  unless a future raw Codex schema explicitly supplies a creation field.
- A Codex fork/subagent rollout may replay parent token history with timestamps
  rewritten to the fork creation second. Detect this only when the first 16
  KiB contains `thread_spawn` or `forked_from_id` and the first two valid token
  snapshots share a timestamp second. Skip every token event in that second,
  clear its pending tool evidence, and keep advancing `total_token_usage` as
  the cumulative baseline for the first real event.
- Replay detection runs only for a parse starting at byte zero. Incremental
  appends continue from the persisted file cursor and must not reapply the
  prefix filter.
- Claude dedupes by `message.id + requestId`; sidechain replay can match by
  message id, prefers non-sidechain metadata, and merges streaming channel
  maxima.
- Codex copied events use a host-plus-source logical identity derived from
  timestamp, normalized model, and the normalized token tuple. Schema v23
  prefixes persisted keys with `{host_id}:` (existing rows use `local:`).
  The same artifact imported from two registered hosts is two events.
- OpenCode uses `max(valid tokens.total, input + cache write + cache read + output)`.
- Kimi Code maps `inputOther`, `inputCacheRead`, `inputCacheCreation`, and
  `output` once each; it has no upstream total or reasoning channel, so its
  total is their saturating sum. Only explicit turn-scoped usage records count.
- Pi and Oh My Pi map `input`, `cacheRead`, `cacheWrite`, and `output` once
  each. A positive `totalTokens` is authoritative; otherwise the four visible
  channels form the fallback total. `reasoningTokens` is persisted separately
  and never added to output or total by default. After the Oh My Pi split, an
  unbounded normal sync treats persisted Pi marker `2` as legacy: it resets
  local `pi` rows and replays `.pi` as `pi` and `.omp` as `omp`.
  `sync --source omp` must refuse while Pi is still legacy. The first remote
  `Omp` shard for a host resets that host's `pi` rows and writes
  `omp_split_migrated.<host_id>` in the same write transaction.
  When `message.usage.cost.total > 0`, persist that USD as
  `source_reported`. `total == 0`, a missing `cost`, and a non-object `cost`
  use the catalog path and are not treated as free. Do not add `pi`/`omp`
  rows to the embedded catalog. Recompute skips `UPDATE` for
  `source_reported` rows and still folds persisted costs into buckets.
  Historical cost backfill is `sync --rebuild --source omp`; do not bump the
  token-accounting version for this cost path.
- Grok maps each `turn_completed` `params.update.usage` object to one event.
  `totalTokens` is authoritative; if it is missing, fall back to
  `inputTokens + outputTokens`. `inputTokens` is cache-inclusive: subtract
  `cachedReadTokens` and `cacheCreationTokens` with saturating clamp to 0.
  Keep `outputTokens` verbatim (reasoning is already inside it). Persist
  `reasoningTokens` as the diagnostic channel and do not add it to total.
  If a session emits any usage event, do not add `_meta.totalTokens` deltas or
  `signals.json` reconciliation. Sessions with no usage keep the total-only
  fallback (subchannels stay zero). Grok has no pricing row: events remain
  `unpriced`. Do not read or convert `costUsdTicks`.
- ZCode `input_tokens` is cache-inclusive. Subtract cache read and cache
  creation from input. Keep `output_tokens` verbatim (reasoning is already
  inside it). Persist `reasoning_tokens` as the diagnostic channel. Trust
  `computed_total_tokens` when present; otherwise fall back to
  `provider_total_tokens`, then the channel sum.
- Antigravity CLI maps `input = #2 + #1`, `cache_read = #5`, `output = #9`
  (text only), and `reasoning = #10`. The `#3 == #9 + #10` checksum proves
  thinking tokens are disjoint from text output, so the total is the channel
  sum including reasoning. There is no authoritative grand total.
- DeepSeek Harness `inputTokens` already excludes cache. Map cache read and
  cache write once each. Keep `outputTokens` verbatim. Persist
  `reasoningTokens` as the diagnostic channel and do not add it to total.
  Total is `input + cache_read + cache_creation + outputTokens`.
- Pricing receives normalized channels. Prompt-tier selection remains
  `input + cache_read + cache_creation`.
- An unbounded normal sync discovers legacy sources only within its selected
  parser set. It emits a repair-started event, preflights every target before
  any reset, resets only the legacy subset, then drives every selected parser
  once under the existing fenced Store.
- Any lossy target blocks every automatic reset for that normal sync. The
  automatic policy ignores `allow_lossy_rebuild` even if a library caller
  constructs inconsistent options.
- A bounded `recent_days` sync with legacy sources fails before reset and
  directs the caller to run unbounded sync first. Resetting full history while
  applying one shared recent cutoff is forbidden.
- The normal-sync repair-finished event is emitted only after writer finish,
  current markers, and source statuses succeed. Failure or cancellation leaves
  legacy markers absent and emits no repair-finished event.
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
| Source has rows and no/currently different marker; unbounded normal sync; lossless inputs | Warn, reset only selected legacy sources, parse selected sources once, then advance successful markers |
| Any selected legacy source has lossy rebuild risk | Refuse before every automatic reset; preserve all rows and markers |
| Bounded normal sync selects a legacy source | Refuse before reset and direct the caller to unbounded sync |
| Source has no rows and no marker | Allow first sync; write marker only after success |
| Rebuild has missing source files | Existing lossy-rebuild guard refuses it |
| Rebuild parser/store commit fails | Leave marker absent; do not claim parity |
| Parserless source | Do not invent a marker or token normalization |
| Persisted Codex marker is `2` | Treat only Codex as legacy and automatically repair it during safe unbounded normal sync |
| Persisted Grok marker is `2` | Treat Grok as legacy and automatically repair it during safe unbounded normal sync |
| Persisted Claude/OpenCode marker is `2` | Treat it as current |
| Persisted Pi marker is `2` | Treat Pi as legacy and automatically repair it during safe unbounded normal sync |
| Persisted Kimi Code/Omp/ZCode/Antigravity/DeepSeek Harness marker is `2` | Treat it as current |
| `sync --source omp` while Pi is legacy | Refuse before any omp writes; direct the caller to unbounded `llmusage sync` |
| Replay marker exists and first two token snapshots share a second | Skip that second's prefix while retaining the latest cumulative baseline |
| Two ordinary Codex requests share a second without a replay marker | Keep both events |
| A malformed line contains `token_count` before valid replay snapshots | Ignore the malformed line and continue detection |
| Serve finds safe legacy parser source | Rebuild before binding the dashboard port |
| Serve finds lossy legacy parser source | Warn, preserve history and marker state, continue startup |
| Serve repair risk query or safe rebuild fails | Return the error and do not bind the port |
| Full rebuild includes parserless history | Preserve it; reset only parser registry sources |

Never enable `--allow-lossy-rebuild` automatically.

## 5. Good / Base / Bad Cases

- Good: Codex raw input `100`, cached `40`, output `30`, reasoning `10`, total
  `130` persists as input `60`, cache read `40`, output `30`, reasoning `10`,
  total `130`.
- Good: a fork prefix contributes `75,064` non-cached input, `381,440` cache
  read, and `3,629` output before the first real event; all three replayed
  components are excluded after rebuild, matching ccusage.
- Base: Codex reports cache read but zero cache creation because OpenAI prompt
  caching does not expose a cache-write token counter.
- Bad: relying only on the logical event key to dedupe fork history. Fork
  rollouts rewrite timestamps, so the copied tuples no longer match the parent
  event keys.
- Base: OpenCode without `tokens.total` falls back to known non-reasoning
  components.
- Bad: a report computes `input + cache + output + reasoning` instead of
  summing persisted `total_tokens`.
- Good: serve repairs Codex, Claude, and OpenCode in registry order while an
  unrelated parserless Antigravity archive remains untouched.
- Good: normal sync repairs safe legacy Codex while current Claude keeps its
  incremental cursor and each selected parser runs exactly once.
- Bad: one lossy source is discovered after another source was already reset,
  or normal sync honors an inconsistent `allow_lossy_rebuild=true`.
- Bad: bounded sync resets a legacy source and advances its marker after
  importing only the requested time window.
- Base: an already-current or empty parser source makes serve repair a no-op.
- Bad: a full rebuild calls `reset_usage_data`, deleting parserless history
  that no registered parser can reconstruct.
- Good: a Grok `turn_completed.usage` row with input `1000`, cache read `400`,
  output `50`, reasoning `20`, total `1050` persists as input `600`, cache
  read `400`, output `50`, reasoning `20`, total `1050`.
- Bad: treating `params._meta.totalTokens` or `signals.contextTokensUsed` as
  request usage when the session also has `turn_completed.usage`. Those fields
  are context-window occupancy and must not be added to the session total.

## 6. Tests Required

- Parser unit tests assert exact integer channel values and total fallbacks.
- Kimi, Pi, Oh My Pi, and Grok parser tests assert raw/future model preservation, authoritative
  versus fallback totals, reasoning isolation, malformed-row tolerance, and
  saturating channel sums. Pi and Oh My Pi `event_key` prefixes (`pi:` / `omp:`)
  must differ and stay idempotent. Pi / Oh My Pi parser tests also cover
  `usage.cost` mapping, missing/non-object cost, and `total == 0` fallthrough.
- Codex parser tests cover both replay markers, cumulative baseline retention,
  pending-tool clearing, malformed-line tolerance, and ordinary same-second
  events that must remain.
- Accounting marker tests assert Codex `3`, Grok `3`, Pi `3`, Claude/OpenCode/Omp `2`, old
  Codex `2` automatic repair to `3`, old Grok `2` automatic repair to `3`, old
  Pi `2` automatic repair to `3`, and successful explicit guarded rebuild.
- `tests/sync/accounting.rs` covers all three sources, copied/streaming
  duplicates, event/bucket/query equality, cost tolerance `1e-9`, marker
  advancement, automatic repair lifecycle, mixed current/legacy behavior,
  bounded refusal, warning payload, and guarded rebuild.
- Automatic normal-sync tests cover multi-source registry order, exactly-once
  parsing, all-target preflight, lossy opt-in isolation, parserless
  preservation, and no completion marker/event after failure.
- `tests/sync/lifecycle.rs` plus `tests/sync/sources/` keep hot sync, append,
  replacement, and rebuild
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

For Codex fork replay:

### Wrong

```rust
// Timestamp-based logical dedupe alone cannot identify rewritten fork history.
events.push(token_event);
```

### Correct

```rust
if replay_marker_in_head && first_two_token_snapshots_share_second {
    skip_creation_second_prefix_and_keep_cumulative_baseline();
}
```

The marker plus same-second gate matches ccusage without dropping ordinary
same-second requests.

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

For Grok Build request usage:

### Wrong

```rust
let total = meta_total_tokens.max(signals.context_tokens_used);
```

### Correct

```rust
if session_has_turn_completed_usage {
    emit_one_event_per_usage_object();
} else {
    fallback_total_only_meta_and_signals();
}
```

`params.update.usage` on `turn_completed` is request usage. `_meta.totalTokens`
and `signals.contextTokensUsed` are context occupancy. Do not mix the two
paths in one session.
