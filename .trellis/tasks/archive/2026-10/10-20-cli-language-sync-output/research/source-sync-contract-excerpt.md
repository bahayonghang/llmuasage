# Source sync contract excerpt for CLI language/output planning

Baseline source: `.trellis/spec/llmusage/backend/source-sync-contracts.md` at 2b70e3f; captured 2026-10-20. Original line ranges below refer to that baseline. Current implementation refinements appended at the end take precedence for presentation. This is curated context, not a replacement spec. The full spec is larger than the 32 KiB injection limit. These ranges cover metrics, engine/adapter ownership, progress/stream/recovery and diagnostics/privacy. Read the original for any additional touched domain.

## Original lines 1-30

# llmusage Source Sync Contracts

## Scenario: Parser Stats And Monitor-Only Platforms

### 1. Scope / Trigger

- Trigger: changes to `SourceKind`, parser `SourceSyncStats`, `source-status`,
  `sync` summaries, `Dashboard::sync_command_center`, or TUI source/sync panels.
- Source parser changes are cross-layer: parser output flows through sync driver
  status, SQLite-derived query payloads, CLI summaries, docs, and TUI rendering.
- Platform monitoring is not parsing. A platform can be detected and shown as
  monitor-only without adding a stable `SourceKind` or importing token rows.
- Usage import is passive-only. Source descriptors do not carry hook/plugin
  activation or integration capabilities, and `init` never installs them.

### 2. Signatures

- Parser runtime stats: `SourceSyncStats { files_processed, changed_files,
  skipped_files, events_emitted, stored_events }`.
- Query/TUI payload: `SyncSourcePayload { files_processed, changed_files,
  skipped_files, stored_events, malformed_lines, oversized_lines,
  skipped_lines, accounting_anomaly_lines, ... }`. Counters only; parse-issue
  samples never enter interactive dashboard JSON.
- Store status rows persist existing source status columns. Do not add a schema
  migration for derived skipped counts unless a consumer needs historical
  skipped totals independent of the latest source sync status.
- Schema v15 adds `source_cursor.last_part_rowid` and
  `(source, source_path_hash)` indexes on `usage_turn` and `usage_tool_call`.
  OpenCode owns the part cursor; file-backed sources continue using `FileCursor`.
- Schema v22 adds `source_cursor.last_skipped_at` and

## Original lines 55-67

  per-product timers. Private profiling reports complete transaction time
  separately. `parse_ms` is a residual elapsed value, not parser CPU time;
  `scanned_bytes` is reader-specific logical input accounting, not disk I/O.
- Claude logical dedupe is scoped to the first directory below
  `~/.claude/projects`. If any file in a project changes, replay every current
  JSONL in that project, but do not replay other projects or reset missing
  historical paths. Projects parse independently; outputs from one bounded
  parallel batch may share one atomic `SyncShard` commit.
- Codex remains file-cursor incremental: unchanged files are metadata-only and
  append work reads only bytes after the stored offset.
- Kimi Code imports only explicit `type=usage.record` plus
  `usageScope=turn` rows. It preserves the raw model id and maps non-cached
  input, cache read, cache creation, and output as separate channels.

## Original lines 126-151

  `crate::commands::sync`.
- A `recent_days` run uses one UTC cutoff. File-backed sources must filter by
  normalized event time when metadata cannot safely exclude a file. OpenCode
  must apply the cutoff in its SQLite page queries. Bounded runs may reuse an
  existing full-history cursor as a lower bound, but must not advance that
  cursor or execute whole-file resets; a later full sync must still recover
  window-excluded history.
- Monitor-only platforms must surface as diagnostics/status entries with token
  quality labels, not as parser-backed usage, until sanitized fixtures and token
  semantics exist.
- A persisted source descriptor without a parser must surface as
  `historical_only`, never `passive_ready` or `passive_no_data`. Historical
  events remain queryable and dashboard filters remain valid, but sync writes
  no new events.
- Antigravity CLI (`antigravity`) and IDE (`antigravity_ide`) share a native
  SQLite decoder. Discover only `.gemini/antigravity-cli/conversations/*.db`
  and `.gemini/antigravity-ide/conversations/*.db`, under `GEMINI_CLI_HOME`
  when set. `.pb`, old/backup roots, credentials, RPC and transcript bodies
  are outside this reader. Verified trajectory source metadata determines
  product ownership (17 CLI, 1 IDE), even when a DB is copied across roots.
  Resolve identities across both roots before applying source filters.
- Native generation usage can aggregate all `retry_infos`: prefer usable
  attempt records and deduplicate generation/step mirrors. Do not sum the
  aggregate with attempts or merge unrelated requests by token amounts/time.
  Only typed generation/step timestamps are request time; file mtime and
  ChatStartMetadata context-window bytes are never time fallbacks.

## Original lines 200-230

  change replays the session family; a bounded run must not reset or advance
  cursors.
- `sync --rebuild --source <source>` must reject a persisted source without a
  registered passive parser even when `--allow-lossy-rebuild` is present. It
  must never delete historical-only events that no parser can reconstruct.
- Sync emits `BootstrapStarted` before lock acquisition for immediate feedback,
  then emits `LockWaiting` / `LockAcquired`; migration and pricing progress run
  only after acquisition on the fenced Store. Existing migration events keep
  their names and meaning; embedded pricing upgrades add
  `pricing_upgrade_started`, `pricing_upgrade_progress`,
  `pricing_bucket_reconcile_started`, and `pricing_upgrade_finished` before
  parser source events.
- Safe legacy accounting repair adds
  `token_accounting_repair_started` before targeted resets and
  `token_accounting_repair_finished` only after writer, marker, and source
  status success. These are additive lifecycle events shared by human stderr,
  NDJSON, TUI, and Web jobs; failure/cancellation remains terminal through the
  existing events.
- After the local parser driver, `llmusage sync` serially imports each
  `transport='ssh'` host through `RemoteImporter` and `commit_shard` on the
  same fenced Store. One host failure records `last_error`, emits
  `RemoteHostSkipped`, and continues. Process exit stays success when local
  sync succeeded. `remote sync [--host <label>]` uses the same importer and
  does not run the local driver.
- Remote shard Header (`SHARD_PROTOCOL_VERSION` 2) carries
  `source_accounting_versions` for every source in that stream. `sync
  --emit-shards` lists the registry parsers selected for the run. The
  decoder owns wire/structure version. The importer validates token
  accounting after Header and before the first related shard commit. Same
  wire protocol does not mean the same token semantics. Old protocol
  streams fail with an explicit mismatch; do not guess accounting from

## Original lines 287-332

  event entry and one copy source (`human_progress_line`). TTY stderr renders
  indicatif bars (OpenCode is a spinner because its `files_scanned` counts
  rows, not files; Codex/Claude use determinate bars whose length and position
  both count files planned for replay in the current run). File-backed parser
  workers increment one relaxed atomic counter per completed file; the async
  parser side samples at no more than 5 Hz, emits a boundary snapshot before
  commit, and refreshes committed record counts after commit. A full TTY bar
  shows the commit phase until `SourceFinished`; non-TTY or any non-empty
  `LLMUSAGE_PROGRESS` falls back to plain lines and must never emit ANSI
  escapes. Progress stays on stderr, the `Sync finished` summary table stays
  on stdout, and renderer
  teardown is owned by a command-level RAII guard so early `?` returns,
  failures, and Ctrl-C cancellation all leave a clean terminal. CLI Ctrl-C
  cancels through `run_once_with_cancel`'s token; a ctrl-c task that clones
  the event sender must be aborted and awaited before the reporter channel is
  relied on to close.
- The interactive TUI is a synchronous renderer running inside the process
  Tokio runtime. It must submit sync work through the in-process `JobRegistry`;
  it must never create a nested runtime or call `block_on` from the render
  thread. A second sync action requests cancellation instead of spawning a
  second job. TUI exit cancels an active job and waits only for a documented,
  bounded interval before restoring the terminal.
- The human `Sync finished` block is a closed ten-column table (files/changed/skipped/
  seen/committed/stored plus human-readable bytes and parse/write durations)
  rendered by the pure `format_summary_lines_with_basenames` (tests may call
  `format_summary_lines`, which is the same formatter with an empty path map);
  coloring is stdout-TTY-only and applied after width computation. The table
  contains only the header, aligned data rows, and an aggregated `TOTAL` row;
  diagnostic lines (`↳`, parse issues, samples, warnings, and errors) appear
  strictly after the bottom border, never between table data rows. These notices
  are grouped by source in table order, omitting private paths, path hashes,
  and duplicate tracing text. Protected or blocked sources show `COMMITTED = 0`,
  retained `STORED`, `CHANGED = 0`, `BYTES = 0`, and `WRITE = 0`; `FILES` shows
  the discovered file count from read-only coverage. `SourceFinished` closes
  live stderr progress without emitting a second permanent success sentence;
  failures and cancellation remain diagnostic lines. Narrow rendering may
  truncate only the display label, never numeric cells. `SyncEvent`/`SourceSyncStats`
  wire shapes are unaffected by display changes.
- Interactive recovery prompting: when stdin, stdout, and stderr are all
  terminals, without `--json-events`, `--rebuild`, or `--recent-days`, and read-only
  coverage detects recoverable loss for `antigravity` or `antigravity_ide`, the user
  may interactively accept loss (`r`) or keep history (`k` or empty input/Enter).
  Accepted products are rebuilt in the same worker lock and process via lossy
  unbounded staged snapshot without pre-reset. Non-interactive runs, windowed runs,
  redirected streams (stdout or stderr redirected, or stdin pipe), and non-Antigravity
  legacy sources never prompt and never read stdin.

## Original lines 558-622


- Trigger: any Codex, Claude, Kimi Code, Pi, or Grok JSONL read loop, parse issue
  projection, file cursor update, or blocking parser cancellation change.
- The shared reader owns byte bounds, JSON decoding, durable record boundaries,
  privacy-safe issues, and cancellation polling. Source parsers own only the
  decoded JSON-to-domain mapping.

### 2. Signatures

- `BoundedJsonlReader::new(reader, start_offset)` uses a 4 MiB maximum record
  size; tests may use `with_limit` for smaller boundaries.
- `read_json_records(source, path_hash, cancel, issues, callback)` passes
  `JsonlRecord { start_offset, end_offset, value }` to the source callback.
  `read_json_records_with_oversized` additionally exposes the bounded 4 MiB
  prefix so a parser can recover or reclassify an oversized record.
- Domain-owned `ParseIssues { malformed_lines, oversized_lines, skipped_lines,
  accounting_anomaly_lines, samples }` is embedded in `SourceSyncStats` and
  `SourceSyncStatus` with serde defaults for the two new counters. Parser
  modules may re-export it but storage must not depend on parser modules.
  `total()` is malformed + oversized (faults). `informational_total()` is
  skipped + accounting_anomaly. Record diagnostics make doctor warn only when
  `total() > 0`; a separate source failure also makes doctor warn.
- `ParseIssueSample` includes `reason` (serde default empty). `record`
  sanitizes reason to at most 64 characters in `[A-Za-z0-9_:-]`.
- `ParseIssueKind` is `malformed`, `oversized`, `skipped`, or
  `accounting_anomaly`. The four classes are mutually exclusive.
- Schema v17 persists the latest bounded diagnostic payload in
  `source_sync_status.parse_issues_json TEXT NOT NULL`.
- A crate-private persisted DTO flattens the existing `ParseIssues` fields and
  adds optional `source_issues`. Each source issue has a closed-set code, count,
  UTC observation time, and product-group scope. It contains no raw path,
  record body, or free-text error. Keep public diagnostic and sync-status
  struct literals compatible; no schema migration is required.

### 3. Contracts

- The reader searches for newlines through `BufRead::fill_buf`; it buffers at
  most 4 MiB of record content and discards the rest of an oversized record in
  bounded chunks through the next newline.
- `complete_offset` advances only after a newline record boundary. A
  syntactically complete JSON value at EOF may still produce an event for
  compatibility, but it cannot advance the durable cursor until its newline is
  observed. A truncated EOF value produces neither an event nor a malformed
  issue.
- Cancellation is checked before each buffered read/discard chunk and between
  files. Async parser loops must await every spawned blocking handle in the
  current batch before returning; a cancelled batch is drained and not
  committed.
- Malformed, oversized, skipped, and accounting-anomaly samples contain only
  source id, bounded path hash, byte offset, issue kind, and an optional
  closed-set reason. Raw JSON, prompts, assistant content, full paths,
  `error_message`, and raw row ids are forbidden in samples, human summaries,
  and logs. CLI sample lines print kind and reason when present. `@offset` is
  printed whenever `offset > 0` for JSONL, including samples with a reason.
  ZCode's existing offset stores a millisecond timestamp, so print
  `timestamp_ms=<offset>` instead. Do not change stored location values or label
  a timestamp as a byte offset. OpenCode's offset is a private database rowid;
  omit that value from human output. Optional
  basename may follow when a file cursor can resolve it. They never print
  `path_hash` or record text.
- At most eight samples are retained per source run. Counters continue with
  saturating arithmetic after the sample budget is exhausted. The human summary
  prints the combined diagnostic count and the omitted sample count. Each
  diagnostic class counts separately; one record can produce multiple accounting
  anomalies. These counters do not count distinct physical records.


## Current implementation refinement

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

### 3. Contracts

Emit notices at existing decision points, using already-observed data. Do not
rediscover inputs, query every file again, or infer state from English/Chinese
error strings. Cancellation feedback belongs to the branch that observes it and
withholds accounting certification, not a token check after successful return.

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
