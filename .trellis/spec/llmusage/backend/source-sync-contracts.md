# llmusage Source Sync Contracts

## Scenario: Parser Stats And Monitor-Only Platforms

### 1. Scope / Trigger

- Trigger: changes to `SourceKind`, parser `SourceSyncStats`, `source-status`,
  `sync` summaries, `Dashboard::sync_command_center`, or TUI source/sync panels.
- Source parser changes are cross-layer: parser output flows through sync driver
  status, SQLite-derived query payloads, CLI summaries, docs, and TUI rendering.
- Platform monitoring is not parsing. A platform can be detected and shown as
  monitor-only without adding a stable `SourceKind` or importing token rows.

### 2. Signatures

- Parser runtime stats: `SourceSyncStats { files_processed, changed_files,
  skipped_files, events_emitted, stored_events }`.
- Query/TUI payload: `SyncSourcePayload { files_processed, changed_files,
  skipped_files, stored_events, ... }`.
- Store status rows persist existing source status columns. Do not add a schema
  migration for derived skipped counts unless a consumer needs historical
  skipped totals independent of the latest source sync status.
- Schema v15 adds `source_cursor.last_part_rowid` and
  `(source, source_path_hash)` indexes on `usage_turn` and `usage_tool_call`.
  OpenCode owns the part cursor; file-backed sources continue using `FileCursor`.
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
- Monitor-only platforms must surface as diagnostics/status entries with token
  quality labels, not as parser-backed usage, until sanitized fixtures and token
  semantics exist.
- Sync bootstrap progress is an observational pre-lock stream. Existing
  migration events keep their names and meaning; embedded pricing upgrades add
  `pricing_upgrade_started`, `pricing_upgrade_progress`,
  `pricing_bucket_reconcile_started`, and `pricing_upgrade_finished` before
  `lock_waiting`.
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
  rows, not files; Codex/Claude use determinate bars whose position counts
  replayed files only); non-TTY or any non-empty `LLMUSAGE_PROGRESS` falls
  back to plain lines and must never emit ANSI escapes. Progress stays on
  stderr, the `Sync finished` summary table stays on stdout, and renderer
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
  rendered by the pure `format_summary_lines`; coloring is stdout-TTY-only and
  applied after width computation. `SyncEvent`/`SourceSyncStats` wire shapes
  are unaffected by display changes.

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

### 5. Good/Base/Bad Cases

- Good: a new monitor descriptor lists candidate roots and parser availability
  while leaving `SourceKind` unchanged.
- Good: one changed Claude file replays only its project, then a bounded group
  of parsed projects shares one writer transaction.
- Base: Codex/Claude/OpenCode parser stats include processed, changed, skipped,
  emitted, and stored counts in CLI JSON/human output and TUI payloads.
- Bad: treating a growing OpenCode DB as replaced because its mtime/length
  changed, or refreshing pricing with one `usage_event WHERE source = ?` scan
  per bucket.
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
- Claude multi-project tests proving unchanged projects remain skipped while
  cross-file streaming/sidechain dedupe inside the changed project is stable.
- Codex append tests asserting only the changed file and appended byte range are
  scanned.
- OpenCode growth/replacement and part high-water tests covering hot zero-row,
  one-row append, closed upper bounds, and idempotent replacement replay.
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
