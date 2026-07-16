# Implementation Plan: Token Accounting Alignment

## Preconditions

- [x] User confirms that ccusage-style total includes each cache channel once;
  the bug is duplicate attribution, not a request to globally remove cache from
  `total_tokens`.
- [x] User approves the legacy-data policy in `design.md`.
- [x] Load `trellis-before-dev` before product edits.
- [x] Re-read `.trellis/spec/llmusage/backend/{source-sync-contracts,pricing-catalog-contracts}.md`.
- [x] Record baseline `git status` and preserve unrelated Trellis upgrade edits.

## 1. Lock The Contract With Failing Tests

- [x] Add `tests/token_accounting_parity.rs` with deterministic Claude, Codex,
  and OpenCode fixtures derived from the reference shapes.
- [x] Assert exact integer equality for input, cache create, cache read, output,
  reasoning, and total. No tolerance is allowed for token counts.
- [x] Add fixed-rate cost assertions with an absolute tolerance of `1e-9`.
- [x] Add the canonical Codex regression:
  raw input `100`, cached `40`, output `30`, reasoning `10`, total `130` ->
  normalized input `60`, cache read `40`, total `130`.
- [x] Add Claude streaming duplicates with the same `message.id + requestId`,
  sidechain replay, and increasing partial token snapshots.
- [x] Add Codex copied/archive/fork records with the same logical token event.
- [x] Add OpenCode rows where `tokens.total` is larger than known components and
  where total is absent.
- [x] Prove the new tests fail on the current implementation for the expected
  reasons before changing production code.

Validation:

```powershell
rtk cargo test --test token_accounting_parity -- --nocapture --test-threads=1
```

## 2. Make Parser Normalization Source-Aware

### Codex

- [x] Split cache aliases into inclusive (`cached_input_tokens`) and explicitly
  separate fields instead of one `explicit_cache_read_tokens` branch.
- [x] Clamp cached input and subtract it from inclusive raw input.
- [x] Preserve upstream per-request total; exclude reasoning from fallback total
  when output is inclusive.
- [x] Keep `last_token_usage` primary and strengthen stale cumulative/fork logic
  with focused cases from ccusage/tokscale.
- [x] Generate stable logical event identity for copied token events.

### Claude

- [x] Parse message id, request id, and sidechain metadata needed by the winner
  rules.
- [x] Coalesce duplicate streaming snapshots by per-channel max, preferring the
  non-sidechain parent when available.
- [x] Reparse changed files from byte zero so appended streaming updates can
  replace prior snapshots.
- [x] Keep reasoning diagnostic-only unless a real fixture proves it disjoint.

### OpenCode

- [x] Parse `tokens.total` and apply `max(authoritative_total, known_components)`.
- [x] Keep cache write/read separate and retain reasoning as a diagnostic field.
- [x] Preserve SQLite high-water and equal-timestamp message-id behavior.

Validation:

```powershell
rtk cargo test parsers::codex -- --nocapture --test-threads=1
rtk cargo test parsers::claude -- --nocapture --test-threads=1
rtk cargo test parsers::opencode -- --nocapture --test-threads=1
```

Review gate:

- [x] Every parser fixture maps raw fields to normalized fields in one place.
- [x] No query/UI module contains provider-specific raw-field interpretation.

## 3. Add Atomic Event Reconciliation

- [x] Replace file-offset-only identity with stable logical identity when the
  source provides enough evidence; retain offset fallback for anonymous rows.
- [x] Reconcile Claude winners before commit by replaying the current inventory
  into one existing `SyncShard`; the existing reset/event/cursor transaction
  subtracts old buckets and inserts winners atomically.
- [x] Handle dedupe-owner rewrite/deletion by rebuilding ownership from all live
  Claude copies whenever a Claude artifact changes.
- [x] Preserve `INSERT OR IGNORE` fast behavior for sources/events that cannot
  replace an existing logical event.
- [x] Cover sync-twice, append, rewrite, copied-file, and missing-owner behavior
  through parity and sync-regression tests.

Validation:

```powershell
rtk cargo test store::sync_writer -- --nocapture --test-threads=1
rtk cargo test sync_regression -- --nocapture --test-threads=1
```

Rollback point:

- [x] Event and bucket reconciliation stays inside the existing `SyncShard`
  commit contract and can be reverted independently of parser normalization.

## 4. Make Persisted Total The Query Authority

- [x] Remove `TokenComponents::total_tokens()` as the report authority.
- [x] Carry persisted event/bucket `total_tokens` through report aggregation.
- [x] Audit and replace component-sum totals in:
  - `src/query/reports.rs`
  - `src/query/logs.rs`
  - `src/query/explorer.rs`
  - `src/query/mod.rs`
- [x] Audit model/source/project/hourly/home-overview payloads for mixed event
  and bucket formulas.
- [x] Stop Web derivations from combining output and reasoning unless the label
  explicitly says it is an expanded diagnostic metric.
- [x] Keep CLI JSON field names and payload shapes stable.
- [x] Add one cross-surface test proving the same seeded events produce the same
  total in daily JSON, daily human output, TUI payload, Web payload, logs, and
  explorer aggregate.

Validation:

```powershell
rtk cargo test --test report_commands -- --nocapture --test-threads=1
rtk cargo test query:: -- --nocapture --test-threads=1
rtk cargo test web:: -- --nocapture --test-threads=1
```

Review gate:

- [x] `rg` finds no unapproved universal formula that adds reasoning or cache to
  a persisted total.
- [x] Event and bucket paths return identical totals for the same filter.

## 5. Recompute Cost From Corrected Channels

- [x] Verify Codex cache reads are charged only at the cache-read rate.
- [x] Keep request-tier selection based on normalized
  `input + cache_read + cache_creation`.
- [x] Verify reasoning follows each catalog row's `ReasoningPolicy` without
  affecting displayed total.
- [x] Reconcile persisted event and bucket costs after event replacement.
- [x] Add same-rate-table parity cases; do not compare against live network
  pricing.

Validation:

```powershell
rtk cargo test query::pricing -- --nocapture --test-threads=1
rtk cargo test pricing_catalog -- --nocapture --test-threads=1
```

## 6. Version Semantics And Handle Existing Databases

- [x] Add per-source `token_accounting_version` markers in `meta`.
- [x] Mark a fresh/successfully rebuilt source current only after commit.
- [x] Detect legacy rows before normal sync and before reports claim parity.
- [x] Expose legacy status in `source-status` and diagnostics warnings.
- [x] Refuse normal writes while legacy rows remain; keep reports read-only with
  an explicit legacy-accounting warning.
- [x] Reuse `sync --rebuild --source` and its lossy guard; never bypass it
  automatically.
- [x] Test fresh DB, legacy DB, successful source rebuild, parser failure,
  commit failure, and missing-source lossy refusal.

Validation:

```powershell
rtk cargo test rebuild -- --nocapture --test-threads=1
rtk cargo test source_status -- --nocapture --test-threads=1
```

## 7. Differential Verification

- [x] Validate the pinned ccusage Rust crate and source-derived equivalent
  fixtures used by llmusage.
- [x] Run focused tokscale Codex/Claude token and dedupe slices and record the
  unrelated Windows failures from its full suite.
- [x] Run llmusage in clean temporary homes and compare comparable integer
  fields exactly to the ccusage contract.
- [x] Compare persisted fixed-rate costs within `1e-9`.
- [x] Record commands and output excerpts in
  `research/parity-verification.md`.

Reference smoke commands may be adjusted to the reference repos' current
package names, but must stay local and pinned:

```powershell
rtk cargo test --manifest-path ref/repo/ccusage/rust/Cargo.toml -p ccusage
rtk cargo test --manifest-path ref/repo/tokscale/Cargo.toml -p tokscale-core
```

## 8. Documentation And Durable Specs

- [x] Update `.trellis/spec/llmusage/backend/` with the executable token
  accounting and semantics-version contract.
- [x] Update `README.md`, `README.zh-CN.md`, and matching docs pages if visible
  totals, warnings, or rebuild steps change.
- [x] Document that cache is counted once in total and separated from Input.
- [x] Document the one-time rebuild path and lossy-rebuild refusal.
- [x] Keep Antigravity explicitly integration-only.

## 9. Final Quality Gate

Targeted tests must pass before the full gate:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo test -- --test-threads=1
rtk npm --prefix docs run docs:build
rtk git diff --check
rtk just ci
rtk python ./.trellis/scripts/task.py validate 07-15-token-accounting-alignment
```

Final review checklist:

- [x] Shared fixture token fields match ccusage exactly.
- [x] tokscale differences are explicit and justified.
- [x] No cache or reasoning subchannel is counted twice.
- [x] Costs use corrected channels and persisted bucket sums.
- [x] Legacy data is never silently presented as corrected.
- [x] Unrelated dirty worktree changes remain untouched.
