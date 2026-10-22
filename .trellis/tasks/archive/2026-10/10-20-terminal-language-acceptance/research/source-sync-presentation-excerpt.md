<!-- Exact first scenario from .trellis/spec/llmusage/backend/source-sync-contracts.md; source SHA256 0e72a6e4bf305f842f8440856ee6472dda605114479ead8858a071276a5e8c7d. Remaining scenarios are outside this feedback-only excerpt. -->

# llmusage Source Sync Contracts

## Scenario: Private Human Feedback Without Data Translation

### 1. Scope / Trigger

Use when adding translated sync progress, recovery explanations, source failures,
or CLI summary notices. Public engine/parser types and persisted diagnostics remain
canonical; language belongs to the presentation adapter.

### 2. Signatures

- `engine::run_once_with_feedback` accepts an optional crate-private `NoticeSink`;
  existing public entry points delegate without one.
- `sync::notices::RunNotice` carries observed coverage, recovery choice, retained
  stock, typed source issues/failures, remote outcomes and actual cancellation.
- Private `SyncFailure { kind: FailureKind, canonical: String }` retains its exact
  original `Display`. `RebuildRisk` retains host, source and protected counts.
- Private `parsers::feedback::collect(Future)` returns the existing result plus
  invocation-local neutral SQLite-open or inventory facts.
- Private `SourceFileListing::error_summary_with_feedback(SourceKind)` returns
  the unchanged canonical summary and emits typed inventory provenance only at
  the six existing parser consumers that assign this summary to `last_error`.
- Private `JobRegistry::with_presentation_observer` borrows notices and typed
  terminal errors before public event stringification, keyed by the existing job
  id. The default presentation path delegates the same locked engine core.
- `RunNotice::RemoteSkipped` carries the original host id and optional private
  importer facts alongside canonical detail. CLI progress and summary consumers
  project these same-call facts without altering the public skipped event.
- Private `RegistryFailure` retains command/status, typed terminal-write error
  and optional original typed failure. Its complete canonical Display has no
  additional Error source, avoiding duplicated persisted error chains.

### 3. Contracts

Emit notices at existing decision points, using already-observed data. Do not
rediscover inputs, query every file again, or infer state from English/Chinese
error strings. Cancellation feedback belongs to the branch that observes it and
withholds accounting certification, not a token check after successful return.

Observers carry neutral facts only; the TUI projects them on its UI thread.
They do not change public executors, events, snapshots, run-log strings, worker
fencing, acquisition order, cancellation/drain policy or remote partial commits.
Remote facts come from the original importer result; contact state, skipped-host
reason, accounting certification and watermarks keep their canonical behavior.
`common::i18n::sync::notice_lines` is the shared pure notice projector; CLI
styling remains in the command adapter and TUI joins the same projected lines.

Parser feedback uses nested Tokio task-local scopes around the same driver
future. Outside a scope emission is a no-op; spawned tasks do not inherit it.
Collection does not add a parser API field, global buffer, locale, extra DB open,
store write, or wire field. OpenCode/ZCode keep their original `last_error` and
exit behavior while the private notice identifies the known SQLite-open failure.

Inventory facts are indexed into the original error list; multi-root merges
adjust indices. Root-not-directory, external IO and traversal context remain
distinct from arbitrary unknown details. Discovery for coverage or ownership
alone emits no failure. Ownership-skip notes remain excluded. Inventory samples
retain the original cap of three and numeric omitted count; parse-issue samples
retain their separate cap of eight. Do not compare `last_error` strings to decide
whether a known inventory notice replaces its human presentation.

Human progress captures the startup language before spawning its reporter. On
failure, drain the reporter before privately rendering typed failure facts.
`SyncEvent::Failed.error`, stored errors and structured file logs retain canonical
strings. Keep original validation order, lock/transaction boundaries, recovery
choices and rebuild safeguards. Typed refusal facts are also available to the
CLI final-error and console-log adapters without a sync-to-commands dependency.

Summary rendering uses the closed-table contract below. Faults
(malformed/oversized) and informational skipped/accounting/absent state remain
distinct; absence uses the existing typed flag. Diagnostics follow the bottom
border, with unchanged numeric cells, sample caps and privacy rules.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Feedback enabled versus disabled | Same imports, cursors, accounting and retained history |
| Known refusal, en/zh, logging off/error | Localized human progress/final/log text; canonical machine/file errors |
| OpenCode/ZCode SQLite open fails | Localized known context; original stats and success exit preserved |
| Nested/concurrent/dropped collector | Independent facts; previous scope restored |
| Token cancelled after completed decision | No invented cancellation notice |
| `--json-events` / `--emit-shards` | Original serialization; no human text on machine stdout |

### 5. Good/Base/Bad Cases

- Good: an existing DB-open error emits a neutral hint and returns the same stats.
- Base: direct public parser/engine calls work without any feedback consumer.
- Bad: translating persisted `last_error`, parsing it to recognize a known error,
  or changing `SyncEvent` solely to carry presentation text.

### 6. Tests Required

- `optional_feedback_preserves_imports_cursors_accounting_and_kept_history`
  compares exact positive-import snapshots, excluding only named audit wall times.
- `nested_collectors_and_concurrent_tasks_are_isolated_and_restored` and
  `dropped_or_failed_collection_restores_scope_and_direct_emit_is_noop` cover scope
  lifecycle, no inheritance and error/cancellation cleanup.
- `bilingual_lossy_refusal_translates_all_human_sinks_and_keeps_canonical_wire_error`
  exercises real temporary history and all three human failure sinks.
- `bilingual_sqlite_open_failure_keeps_success_exit_and_canonical_stats_and_shards`
  covers both producers and protected canonical output.
- `consumed_inventory_summary_has_typed_provenance_cap_and_unknown_details` and
  `paired_inventory_diagnostics_keep_cap_counts_and_additional_unknown_detail`
  check consumption-only feedback, original summaries, cap/omitted counts and
  retained arbitrary details. `bilingual_inventory_root_fault_is_localized_without_private_path_or_wire_changes`
  exercises the actual regular-file root and canonical machine records.
- Existing accounting/rebuild, recovery input/stream matrix, JSON/shard, progress
  interleaving, terminal cleanup, Unicode table and bounded sample tests remain required.

### 7. Wrong vs Correct

Wrong: use `stats.last_error.contains("打开失败")` to choose an English sentence.
Correct: emit `SqliteOpenFailed { source }` at that existing producer, keep stats
unchanged, and let the human adapter choose its language.
