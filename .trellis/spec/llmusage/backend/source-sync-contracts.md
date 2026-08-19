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
  `source_cursor.last_skipped_ids_json`. ZCode owns the skip watermark;
  file-backed sources and OpenCode do not read or write these columns. Do not
  reuse `last_processed_ids_json` or `last_total_json` for skip diagnostics.
- Stable passive parser ids include `kimi_code` for
  `~/.kimi-code/sessions/**/wire.jsonl` and one `pi` id for both
  `~/.pi/agent/sessions` and `~/.omp/agent/sessions`. Grok Build uses `grok`
  for direct sidecars under `~/.grok/sessions/*/*/` or `GROK_HOME/sessions`.
- Registered passive parsers are Codex, Claude, OpenCode, Antigravity, Kimi
  Code, Pi, Grok Build, ZCode, and DeepSeek Harness.
- Monitor descriptors live outside parser promotion and report detection status,
  candidate roots, and parser availability.

### 3. Contracts

- `skipped_files` means known source artifacts that were seen but not reparsed
  because their fingerprint/cursor state did not require importing events.
- `files_processed` counts source artifacts considered by the parser for that
  run, not rows committed to `usage_event`.
- `changed_files` counts artifacts that produced new or refreshed parser work.
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
- Pi and Oh My Pi share one source id and one source-status row. Discovery
  merges canonical files across both roots (and comma-separated
  `PI_AGENT_DIR` roots), then uses the ordinary append/reparse `FileCursor`
  state machine. Assistant usage keeps the upstream total authoritative and
  reasoning diagnostic-only.
- Grok Build discovery uses exactly two `read_dir` levels for
  `sessions/<workspace>/<session>` and only joins the whitelisted direct
  sidecars `updates.jsonl`, `signals.json`, `summary.json`, and optional
  `events.jsonl`; it never uses recursive `WalkDir` discovery. Any sidecar
  change reparses the complete session and resets one shared session path hash.
  A tracked missing sidecar preserves prior events and lets the ordinary
  source-file sweep plus lossy-rebuild guard own recovery until it returns.
- OpenCode database replacement detection uses persisted message anchors
  `(last_time_created, last_processed_ids)`. Preserve all cursors when every
  anchor exists; if any anchor disappeared, reset message and part cursors.
  File size, mtime, head signatures, and Windows creation time are not database
  generation identities.
- OpenCode tool parts use a persisted `last_part_rowid`. Read pages only inside
  `(last_part_rowid, MAX(rowid)]`, advance after the closed range completes,
  and leave the cursor unchanged on cancellation/failure. A missing `part`
  table degrades to no tool rows.
- Writer reset paths must be set-oriented where cardinality amplifies work:
  behavior deletes use a temporary path-key table, and reset bucket pricing is
  recomputed with one source-range event scan joined to a temporary bucket-key
  table. Never issue one source-range event scan per touched bucket.
- `stored_events` is the committed event count after store dedupe and reset
  behavior; it can be lower than parser-emitted raw events.
- Sync request validation has one owner: `ValidatedSyncRequest`. CLI, Web, and
  public `JobRegistry::try_start` must return `unknown_source`,
  `invalid_recent_days`, or `invalid_parallelism` before creating work.
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
- Antigravity is parser-backed for CLI `conversations/*.db`. Hook-era rows
  with an empty `source_path_hash` stay queryable. `sync --rebuild` that
  includes Antigravity must refuse when any such unattributed row exists, even
  when `--allow-lossy-rebuild` is present.
- ZCode reads `~/.zcode/cli/db/db.sqlite` `model_usage` completed rows with a
  `completed_at` high-water cursor. Unfinished `error`/`cancelled` rows are
  counted as `skipped` against both that completed watermark and a separate
  skip watermark (`last_skipped_at` + `last_skipped_ids`). A row is reported
  only when it is new relative to both watermarks. A full uncancelled run
  advances the skip watermark to this batch's newest unfinished
  `completed_at` and the ids at that timestamp. A bounded run may reuse both
  cursors as lower bounds but must not advance either. Cancellation must not
  persist the skip watermark, including after a completed page save. Missing
  completed anchors reset both watermarks. Unfinished rows are never imported
  as `UsageEvent`.
- DeepSeek Harness discovers `$DSH_HOME` (default `~/.dsh`) `sessions/` at any
  depth for files named exactly `session.jsonl` or `session.jsonl.zstd`.
  Compression is dispatched by zstd frame magic. An unbounded fingerprint
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
- Pricing started/progress events carry source/target catalog versions and
  processed/total event counts. Reconcile/finished events carry bucket counts;
  finished also carries deleted orphan count and elapsed milliseconds.
- Human stderr and `sync --json-events` consume one bootstrap-to-sync mapping.
  Human output may replace a TTY line but must end lines at reconcile/finished
  boundaries. JSON mode keeps stdout NDJSON-only and treats pricing variants as
  additive. No-op or pinned catalog bootstrap emits no pricing variants.
- Bootstrap callback delivery must not persist progress or alter migration,
  pricing activation, lock acquisition, failure, or cancellation semantics.
- Human progress rendering lives in `src/commands/sync_progress.rs` behind one
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
- The human `Sync finished` block is an aligned table (files/changed/skipped/
  seen/committed/stored plus human-readable bytes and parse/write durations)
  rendered by the pure `format_summary_lines_with_basenames` (tests may call
  `format_summary_lines`, which is the same formatter with an empty path map);
  coloring is stdout-TTY-only and applied after width computation. It ends with a `TOTAL` row aggregated from
  per-source stats. `SourceFinished` closes live stderr progress without
  emitting a second permanent success sentence; failures and cancellation
  remain diagnostic lines. Narrow rendering may truncate only the display
  label, never numeric cells. `SyncEvent`/`SourceSyncStats` wire shapes are
  unaffected by display changes.

### 4. Validation & Error Matrix

- Missing sanitized fixture -> keep platform monitor-only and document the gap.
- Unknown token semantics -> keep token quality as unsupported/unknown and do
  not compute costs.
- Second unchanged sync -> `skipped_files > 0`, `changed_files == 0`, and
  imported usage remains available.
- Source rewrite or fingerprint change -> artifact leaves skipped state and the
  focused regression must show refreshed parser/store visibility.
- OpenCode growth with all message anchors present -> keep message and part
  high-waters; database replacement with a missing anchor -> reset both.
- OpenCode `part` table absent -> message sync succeeds and part cursor does not
  advance.
- Existing JSON without `skipped_files` -> serde default must load as `0`.
- Omitted source -> `SyncSourceSelection::All`; an unknown source string ->
  `unknown_source` before a job id or sync worker is created.
- `recent_days` outside `1..=3650` -> `invalid_recent_days`; parser parallelism
  outside `1..=32` -> `invalid_parallelism`. Do not clamp either value.
- A bounded run succeeds -> mark `recent_completed_at` and emit
  `RecentReady` only after every requested parser stage and status write
  completes; cancellation/failure emits neither completion signal.

### 5. Good/Base/Bad Cases

- Good: a new monitor descriptor lists candidate roots and parser availability
  while leaving `SourceKind` unchanged.
- Good: one changed Claude file replays only its project, then a bounded group
  of parsed projects shares one writer transaction.
- Base: Codex/Claude/OpenCode parser stats include processed, changed, skipped,
  emitted, and stored counts in CLI JSON/human output and TUI payloads.
- Base: an omitted source selects all registered parsers through the validated
  request; no transport adapter interprets `None` independently.
- Good: a recent event appended to an old JSONL file is imported by a bounded
  run, while the old event remains recoverable by a later full sync.
- Bad: treating a growing OpenCode DB as replaced because its mtime/length
  changed, or refreshing pricing with one `usage_event WHERE source = ?` scan
  per bucket.
- Bad: converting an unknown source to `None`, advancing the full-history
  cursor during a bounded run, or emitting `RecentReady` after only one parser
  when the request selected all sources.
- Bad: adding a Gemini/Cursor/etc. parser ID only because a root directory was
  detected, without token fixtures and cursor/fingerprint tests.

### 6. Tests Required

- Parser stats tests for changed and unchanged sync runs, including
  `skipped_files`.
- Backward-compatible serde/default tests when adding optional stats fields.
- Query/TUI payload tests when a stats field becomes visible in dashboard or
  terminal panels.
- Registry/monitor tests proving monitored platforms do not accidentally become
  parser-backed sources.
- Descriptor/status tests proving parser capabilities match the registry and
  Antigravity remains `historical_only` plus monitor-only
  `blocked_no_samples`.
- Behavior-level rebuild coverage proving a targeted Antigravity rebuild is
  rejected and existing historical rows remain intact.
- Report and dashboard projection coverage proving historical Antigravity rows
  remain aggregated and selectable.
- Kimi, Pi, and Grok fixture tests covering normalized fields, raw/future model ids,
  malformed/non-usage rows, second-sync idempotency, append, rewrite/truncate,
  deleted history/rebuild protection, missing roots, and status projections.
- Sync-summary unit/subprocess tests covering the `TOTAL` row, absent and empty
  sources, ANSI-free redirected output, stderr/stdout separation, removed
  completion sentences, and narrow/wide column budgets.
- Claude multi-project tests proving unchanged projects remain skipped while
  cross-file streaming/sidechain dedupe inside the changed project is stable.
- Codex append tests asserting only the changed file and appended byte range are
  scanned.
- OpenCode growth/replacement and part high-water tests covering hot zero-row,
  one-row append, closed upper bounds, and idempotent replacement replay.
- Table-driven CLI, Web, and public `JobRegistry::try_start` tests asserting
  the same validation codes and no job creation for invalid input.
- Recent-window regressions asserting event-time filtering, an old file with a
  recent append, no bounded cursor/reset writes, later full-history recovery,
  OpenCode SQL lower-bound pruning, and `RecentReady` ordering.
- Migration/query-plan tests proving behavior reset indexes exist; writer tests
  proving shard-local behavior dedupe and shared-bucket pricing recovery.
- Human and subprocess tests covering pricing phase text, ordered additive
  NDJSON variants, stdout purity, and structured log phase fields.
- A multi-thread `#[tokio::test]` covering the TUI sync action, duplicate-start
  cancellation, progress text projection, and bounded shutdown behavior.

### 7. Wrong vs Correct

#### Wrong

```rust
// Root detected, so add a SourceKind and let the dashboard show zero-cost rows.
SourceKind::Gemini
```

#### Correct

```rust
// Root detected, but no trusted token fixture yet: expose monitor-only status.
PlatformMonitorDescriptor {
    id: "gemini",
    parser_available: false,
    token_quality: TokenQuality::Unsupported,
    candidate_roots,
}
```

```rust
// Validate the persisted parser anchor; content metadata is not a DB generation.
if !opencode_cursor_anchor_exists(&connection, &cursor)? {
    cursor.last_time_created = 0;
    cursor.last_processed_ids.clear();
    cursor.last_part_rowid = 0;
}
```

```rust
// Wrong: unknown source silently becomes the all-sources sentinel.
let source = input.source.as_deref().and_then(SourceKind::parse_id);

// Correct: validate transport input once, then pass only typed selection.
let request = ValidatedSyncRequest::new(input)?;
let source = request.source_kind();
```

## Scenario: Bounded Passive JSONL Records And Cooperative Cancellation

### 1. Scope / Trigger

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
  skipped + accounting_anomaly. Doctor warns only when `total() > 0`.
- `ParseIssueSample` includes `reason` (serde default empty). `record`
  sanitizes reason to at most 64 characters in `[A-Za-z0-9_:-]`.
- `ParseIssueKind` is `malformed`, `oversized`, `skipped`, or
  `accounting_anomaly`. The four classes are mutually exclusive.
- Schema v17 persists the latest bounded diagnostic payload in
  `source_sync_status.parse_issues_json TEXT NOT NULL`.

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
  printed only when `offset > 0` and reason is empty (JSONL). Optional
  basename may follow when a file cursor can resolve it. They never print
  `path_hash` or record text.
- At most eight samples are retained per source run. Counters continue with
  saturating arithmetic after the sample budget is exhausted.
- Codex classifies an oversized prefix from the first 8 KiB of payload/msg
  type: other types are skipped; a complete `token_count` JSON prefix (trailing
  whitespace allowed) is recovered with no issue; a `token_count` prefix that
  cannot be parsed stays oversized. Peek-none (unclassified junk) stays
  oversized, not skipped.
- ZCode `error`/`cancelled` rows are skipped. The unfinished reason is
  `zcode_unfinished:{status}:{error_type}` where status is
  `error`/`cancelled`/`other` and `error_type` comes from
  `model_usage.error_type` when the column exists and matches
  `[A-Za-z0-9_-]`; otherwise `unknown`. Never read `error_message`. Cache
  overlap and `computed_total` mismatch are accounting anomalies; events
  still store.
- Antigravity open/decode/missing timestamp stay malformed. Checksum mismatch
  and missing `response_id` with a fallback key are accounting anomalies.
- Grok sidecars over the size cap stay oversized; bad sidecar JSON stays
  malformed. OpenCode does not invent parse issues.
- Sync human summary prints every non-zero class (`malformed=`, `oversized=`,
  `skipped=`, `accounting=`). Warning color is only for malformed/oversized.
- Interactive dashboard `SyncSourcePayload` carries the four counters and
  never parse-issue samples. Source cards and the TUI Usage wide table show
  non-zero counts from those fields, not by parsing human summary strings.

### 4. Validation & Error Matrix

- Record exceeds 4 MiB and reaches newline -> the wrapper path increments
  `oversized_lines`; a source-specific oversized callback may instead recover
  usage (`Accepted`), count `skipped_lines`, or count `malformed_lines`, then
  advance to that boundary and continue with the next record.
- Complete record is invalid JSON -> increment `malformed_lines`, skip it, and
  continue without failing the file.
- Complete non-usage JSONL rows that a parser ignores are not parse issues.
- ZCode unfinished rows that are new relative to both watermarks ->
  `skipped_lines` plus a reason sample. A later unchanged sync of the same
  unfinished rows -> `skipped_lines == 0`, no samples, and no new
  parse-issue info event. Token-channel inconsistency with a stored event ->
  `accounting_anomaly_lines`.
- EOF contains invalid/incomplete JSON -> keep the prior durable offset and do
  not count malformed until a record boundary exists.
- Cancellation during ordinary read or oversized discard -> stop without a
  cursor for the interrupted file; drain every blocking worker before terminal
  cancellation.
- Persisted issue JSON is invalid -> diagnostics/status loading fails as a
  SQLite conversion error instead of silently inventing clean counters.
- Old `parse_issues_json` without `skipped_lines` / `accounting_anomaly_lines`
  deserializes those counters as `0`. Old samples without `reason`
  deserialize `reason` as `""`.
- Doctor `parse.issues` is `ok` when every source `total() == 0`, even if
  skipped or accounting-anomaly counts are non-zero.

### 5. Good/Base/Bad Cases

- Good: a 10 MiB line yields one oversized issue while the in-memory record
  buffer stays at or below 4 MiB, then the following valid line is parsed.
- Base: valid newline-delimited records advance `complete_offset` and emit the
  same source events as before.
- Good: a valid EOF record is visible immediately but is retried from the prior
  durable boundary; event keys/store dedupe keep the retry idempotent.
- Bad: `BufRead::read_line`, `lines()`, or a source-local `serde_json::from_str`
  loop in a passive JSONL parser.
- Bad: dropping `JoinHandle`s when cancellation is observed; blocking tasks
  continue consuming CPU/I/O after the job reports cancelled.

### 6. Tests Required

- Shared Codex/Claude/Kimi/Pi/Grok contract harness: 10 MiB oversized line,
  malformed line with secret content, UTF-8 record, EOF tail, identical issue
  counters, safe samples, and durable offset. The 10 MiB junk-line prefix
  (`x` bytes) remains oversized, not skipped.
- Reader unit tests: maximum buffered bytes, discard continuation, malformed
  privacy, mid-discard cancellation, start offsets, EOF stability, and
  oversized-prefix reclassification (`Skipped` / recovered / oversized).
- Codex tests: 10 MiB non-`token_count` -> skipped + later rows parse;
  complete `token_count` prefix padded with whitespace -> event and zero
  issues; unusable `token_count` prefix -> oversized + later rows parse.
- Per-source partial-tail/append tests plus `tests/sync_regression.rs` for
  rewrite, retry, idempotency, and stored totals.
- Sync-summary, doctor, and source-status tests covering four-class counters,
  warning color only for faults, CLI samples without `path_hash`/record text
  or `@0` when a reason is present, JSONL `@offset` when reason is empty,
  and doctor skipping skipped-only sources.
- ZCode skip-watermark tests covering first-sighting reasons, a second
  unchanged sync with `skipped_lines == 0`, a newer unfinished row reported
  once, `--recent-days` not advancing the skip watermark, cancel after the
  first page save not persisting the skip watermark, and rebuild resetting
  both watermarks.
- Query/dashboard/TUI tests covering `SyncSourcePayload` four counters, no
  samples in interactive JSON, source-card counts, and Usage Issues column.
- JobRegistry test: status remains `cancelling` and `finished_at` stays absent
  until a blocking worker confirms drain.
- Migration/status tests: v17 default payload and `ParseIssues` round trip,
  including missing new fields deserializing as zero.

### 7. Wrong vs Correct

#### Wrong

```rust
let mut line = String::new();
while reader.read_line(&mut line)? != 0 {
    let Ok(value) = serde_json::from_str(&line) else { continue };
    parse_source_value(value)?;
}
```

#### Correct

```rust
reader.read_json_records(source, path_hash, cancel, &mut issues, |record| {
    parse_source_value(record.value)
})?;
```
