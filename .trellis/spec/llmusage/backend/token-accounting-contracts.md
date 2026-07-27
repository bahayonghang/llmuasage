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
  `meta('token_accounting_version.codex') = '3'`; Claude, OpenCode, Kimi Code,
  Pi, and Grok remain `2`. `expected_token_accounting_version(SourceKind) -> u32`
  owns this source-aware contract.
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
- Codex copied events use a source-scoped logical identity derived from
  timestamp, normalized model, and the normalized token tuple.
- OpenCode uses `max(valid tokens.total, input + cache write + cache read + output)`.
- Kimi Code maps `inputOther`, `inputCacheRead`, `inputCacheCreation`, and
  `output` once each; it has no upstream total or reasoning channel, so its
  total is their saturating sum. Only explicit turn-scoped usage records count.
- Pi maps `input`, `cacheRead`, `cacheWrite`, and `output` once each. A positive
  `totalTokens` is authoritative; otherwise the four visible channels form the
  fallback total. `reasoningTokens` is persisted separately and never added to
  output or total by default.
- Grok maps cumulative update deltas plus the remaining signals reconciliation
  into authoritative `total_tokens`. Input, cache read, cache creation, output,
  and reasoning stay zero because the local artifacts expose no trustworthy
  split. Grok has no pricing row: a total-only event with zero chargeable
  subchannels must remain `unpriced`, never falsely matched at zero cost.
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
| Persisted Claude/OpenCode marker is `2` | Treat it as current |
| Persisted Kimi Code/Pi/Grok marker is `2` | Treat it as current |
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

## 6. Tests Required

- Parser unit tests assert exact integer channel values and total fallbacks.
- Kimi, Pi, and Grok parser tests assert raw/future model preservation, authoritative
  versus fallback totals, reasoning isolation, malformed-row tolerance, and
  saturating channel sums.
- Codex parser tests cover both replay markers, cumulative baseline retention,
  pending-tool clearing, malformed-line tolerance, and ordinary same-second
  events that must remain.
- Accounting marker tests assert Codex `3`, Claude/OpenCode `2`, old Codex `2`
  automatic repair to `3`, and successful explicit guarded rebuild.
- `tests/token_accounting_parity.rs` covers all three sources, copied/streaming
  duplicates, event/bucket/query equality, cost tolerance `1e-9`, marker
  advancement, automatic repair lifecycle, mixed current/legacy behavior,
  bounded refusal, warning payload, and guarded rebuild.
- Automatic normal-sync tests cover multi-source registry order, exactly-once
  parsing, all-target preflight, lossy opt-in isolation, parserless
  preservation, and no completion marker/event after failure.
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
