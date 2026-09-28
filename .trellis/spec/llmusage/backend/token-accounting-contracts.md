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
  `meta('token_accounting_version.pi') = '3'`;
  `meta('token_accounting_version.antigravity') = '3'` and
  `meta('token_accounting_version.antigravity_ide') = '3'`; Claude, OpenCode,
  Kimi Code, Oh My Pi (`omp`), ZCode, and DeepSeek Harness remain `2`.
  `expected_token_accounting_version(SourceKind) -> u32` owns this source-aware
  contract.
- Legacy repair: explicit `llmusage sync --rebuild --source <source>`. Ordinary
  `llmusage sync` does not rebuild. Claude Code, Codex, Grok Build, Kimi Code,
  and Oh My Pi (OMP) must treat ordinary sync and explicit rebuild as different
  commands.
- Ordinary-sync skip: `legacy_token_accounting_sources_for` detects selected
  parser sources, `exclude_legacy_sources_from_write_set` removes them from the
  parser/write set before the driver runs, and
  `SyncStatusStore::legacy_repair_warning` names
  `llmusage sync --rebuild --source <source>` plus the existing
  `--allow-lossy-rebuild` path. Do not emit
  `SyncEvent::TokenAccountingRepairFinished`. Do not claim the source was
  repaired. Do not advance that source's token-accounting marker.
- Serve startup:
  `commands::serve::repair_legacy_token_accounting(&AppContext, &Store) -> Result<TokenAccountingRepairReport>`.
  It detects legacy sources, records them as not rebuilt, warns, and does not
  call sync with `rebuild: true`.
- Repair reports list `rebuilt_sources` and `blocked_sources`; each blocked row
  includes `source`, `missing_file_count`, and `protected_event_count`. Ordinary
  serve startup leaves `rebuilt_sources` empty and lists skipped legacy sources
  in `blocked_sources`.

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
  and never added to output or total by default. After the Oh My Pi split,
  persisted Pi marker `2` is legacy. Ordinary sync keeps existing `pi` rows,
  skips `pi` writes for that round, and warns that repair is
  `llmusage sync --rebuild --source pi`. `sync --source omp` must refuse while
  Pi is still legacy and must point at that explicit rebuild, not unbounded
  ordinary sync. The first remote `Omp` shard for a host resets that host's
  `pi` rows and writes `omp_split_migrated.<host_id>` in the same write
  transaction.
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
- Antigravity CLI and IDE use installed protobuf descriptor semantics:
  `#1` is a model enum, `input = #2`, `cache_creation = #4`,
  `cache_read = #5`, `reasoning = #9`, and visible `output = #10`.
  `#3` is total output and must equal `#9 + #10`; it is not the grand total.
  Normalize total to the sum of the five disjoint token channels. The sum
  alone does not prove channel names; descriptors are the semantic oracle.
  Native `retry_infos` are attempt records and direct usage can be their
  cumulative sum. When attempts are available, use them instead of adding
  direct usage; deduplicate their generation/step mirrors by request identity.
  Migration v21 must keep its historical literal marker `2`. Ordinary sync
  preserves/skips v2; explicit rebuild stages before atomically replacing
  attributed parser rows and setting v3. Unattributed hook-era rows remain
  included in historical totals with a retained-history warning; they are
  not claimed to have been converted to v3. Remote old accounting continues
  to be refused: host-history restoration is not implemented.
- DeepSeek Harness `inputTokens` already excludes cache. Map cache read and
  cache write once each. Keep `outputTokens` verbatim. Persist
  `reasoningTokens` as the diagnostic channel and do not add it to total.
  Total is `input + cache_read + cache_creation + outputTokens`.
- Pricing receives normalized channels. Prompt-tier selection remains
  `input + cache_read + cache_creation`.
- Ordinary sync (bounded or unbounded) discovers legacy sources only within
  its selected parser set. It keeps that source's existing event, raw, bucket,
  turn, tool, cursor, and source_file data, removes the source from the
  parser/write set before driving parsers, emits an explicit-repair warning,
  and does not reset, parse, or mix new accounting into old rows. Non-legacy
  sources in the same run still sync and remain idempotent on a second run.
  Cancel after legacy detect still preserves the skip/keep invariants.
- Ordinary sync ignores `allow_lossy_rebuild` because it no longer rebuilds.
- Ordinary sync must not emit `TokenAccountingRepairFinished` as success and
  must not advance a skipped source's token-accounting marker.
- `llmusage serve` detects legacy parser sources after store bootstrap and
  before binding a port. It keeps and shows existing data plus the accounting
  warning. It does not implicit-rebuild (`rebuild=true`). A broken or
  unparseable legacy source must not stop dashboard startup. Query still sees
  old totals; the warning is visible.
- Lossy and safe legacy sources both preserve history on serve startup. The
  dashboard may start. Normal writes remain skipped for those sources until
  explicit rebuild.
- Explicit `llmusage sync --rebuild` keeps the existing missing-file check
  and `--allow-lossy-rebuild` gate. Do not add auto-backup, staging DB,
  compatibility framework, or optional config flags.
- A no-source full rebuild derives its preflight, reset, marker-clear, and
  parser fan-out boundaries from the same parser collection. Ordinary parser
  sources use the source reset path. Antigravity CLI/IDE bypass pre-reset and
  use `SyncWriter::commit_antigravity_snapshot(shards, rebuild)` after staging.
  Both paths preserve parserless events, buckets, behavior facts, cursors and
  source-file state; the native snapshot also preserves unattributed hooks.
- `Store::reset_usage_data` is a low-level global reset surface. Command-level
  full rebuild must not call it because it has no parser capability boundary.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Source has rows and its expected source-specific marker | Normal incremental writes are allowed |
| Source has rows and no/currently different marker; ordinary sync (bounded or unbounded) | Keep existing data, skip that source's writes, warn for explicit `llmusage sync --rebuild --source <source>`, do not advance the marker |
| Ordinary sync caller sets `allow_lossy_rebuild=true` | Ignore it; still skip+warn and do not rebuild |
| Source has no rows and no marker | Allow first sync; write marker only after success |
| Rebuild has missing source files | Existing lossy-rebuild guard refuses it |
| Ordinary pre-reset rebuild parser/store commit fails | Leave marker absent; do not claim parity |
| Antigravity staging/transaction fails | Preserve prior rows and marker atomically; do not claim repair |
| Persisted or remote source diagnostics contain a source failure and zero record faults | Retain the failure and refuse clean repair certification; `ParseIssues::total() == 0` alone is insufficient |
| Parserless source | Do not invent a marker or token normalization |
| Persisted Codex marker is `2` | Treat only Codex as legacy; ordinary sync skips it; explicit rebuild repairs it |
| Persisted Grok marker is `2` | Treat Grok as legacy; ordinary sync skips it; explicit rebuild repairs it |
| Persisted Claude/OpenCode marker is `2` | Treat it as current |
| Persisted Pi marker is `2` | Treat Pi as legacy; ordinary sync skips it; explicit `sync --rebuild --source pi` repairs it |
| Persisted Kimi Code/Omp/ZCode/DeepSeek Harness marker is `2` | Treat it as current |
| Persisted Antigravity CLI/IDE marker is `2` | Keep history and skip ordinary writes; require explicit staged rebuild |
| `sync --source omp` while Pi is legacy | Refuse before any omp writes; direct the caller to `llmusage sync --rebuild --source pi` |
| Replay marker exists and first two token snapshots share a second | Skip that second's prefix while retaining the latest cumulative baseline |
| Two ordinary Codex requests share a second without a replay marker | Keep both events |
| A malformed line contains `token_count` before valid replay snapshots | Ignore the malformed line and continue detection |
| Serve finds a legacy parser source (safe, lossy, or unparseable) | Warn, preserve history and marker state, continue startup; do not rebuild |
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
- Good: serve finds Codex, Claude, and OpenCode legacy, keeps their history,
  lists them as not rebuilt, and still starts the dashboard. An unrelated
  parserless archive remains untouched.
- Good: ordinary sync skips safe legacy Codex, keeps its rows and marker, warns
  for `llmusage sync --rebuild --source codex`, and still incrementally syncs
  current Claude. A second ordinary run stays idempotent.
- Good: ordinary sync with a missing marker and an unparseable source fixture
  leaves event/raw/bucket/turn/tool/cursor/source_file content unchanged.
- Bad: ordinary sync resets a legacy source, parses it, or emits
  TokenAccountingRepairFinished as success.
- Bad: bounded sync resets a legacy source and advances its marker after
  importing only the requested time window.
- Bad: serve sets `rebuild=true` or stops dashboard startup because a skipped
  legacy source is unparseable.
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
- Antigravity's sanitized native fixture test asserts every channel against
  an independent descriptor-based oracle. Labeled constructed tests cover
  positive cache creation and thinking-only/visible-only output; do not claim
  those positive shapes were observed in the native corpus.
- Native busy/cancel regressions start with positive imported history, compare
  full event/bucket/cursor/inventory rows and the old marker, then assert no
  repair certification. A real SQLite exclusive lock must demonstrate BUSY;
  a cancelled snapshot must not be mistaken for a malformed busy failure.
- Kimi, Pi, Oh My Pi, and Grok parser tests assert raw/future model preservation, authoritative
  versus fallback totals, reasoning isolation, malformed-row tolerance, and
  saturating channel sums. Pi and Oh My Pi `event_key` prefixes (`pi:` / `omp:`)
  must differ and stay idempotent. Pi / Oh My Pi parser tests also cover
  `usage.cost` mapping, missing/non-object cost, and `total == 0` fallthrough.
- Codex parser tests cover both replay markers, cumulative baseline retention,
  pending-tool clearing, malformed-line tolerance, and ordinary same-second
  events that must remain.
- Accounting marker tests assert Codex `3`, Grok `3`, Pi `3`, Claude/OpenCode/Omp `2`, old
  Codex/Grok/Pi `2` remaining legacy under ordinary sync, and successful
  explicit guarded rebuild.
- `tests/sync/accounting.rs` covers all three sources, copied/streaming
  duplicates, event/bucket/query equality, cost tolerance `1e-9`, marker
  non-advancement on ordinary skip, mixed current/legacy skip+sync,
  cancel-after-detect, bounded skip, warning payload, serve no-reset, and
  guarded explicit rebuild. Drive shipped
  `commands::sync::run_once_with_options` /
  `commands::serve::repair_legacy_token_accounting` / `SyncRunOptions { rebuild: true }`.
- Ordinary-sync skip tests cover unparseable keep, mixed sources, second-run
  idempotence, lossy opt-in isolation, and no repair-finished claim.
- `tests/sync/lifecycle.rs` plus `tests/sync/sources/` keep hot sync, append,
  replacement, and rebuild
  behavior idempotent.
- Serve repair tests assert no reset, query-visible old totals, visible
  warning, lossy and safe history preserved, and unparseable legacy not
  blocking the function. Do not start a real user server.
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
for source in ordinary_pre_reset_sources {
    store.reset_for_source(source)?;
}
// Antigravity stages complete input first; the writer owns reset + insert + marker.
writer.commit_antigravity_snapshot(native_shards, true)?;
```

The correct form cannot delete a parserless source that the subsequent parser
fan-out is unable to reconstruct, or native history before its snapshot is ready.

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
