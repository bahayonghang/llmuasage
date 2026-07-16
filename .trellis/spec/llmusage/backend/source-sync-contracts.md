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
- Monitor descriptors live outside parser promotion and report detection status,
  candidate roots, and parser availability.

### 3. Contracts

- `skipped_files` means known source artifacts that were seen but not reparsed
  because their fingerprint/cursor state did not require importing events.
- `files_processed` counts source artifacts considered by the parser for that
  run, not rows committed to `usage_event`.
- `changed_files` counts artifacts that produced new or refreshed parser work.
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

### 4. Validation & Error Matrix

- Missing sanitized fixture -> keep platform monitor-only and document the gap.
- Unknown token semantics -> keep token quality as unsupported/unknown and do
  not compute costs.
- Second unchanged sync -> `skipped_files > 0`, `changed_files == 0`, and
  imported usage remains available.
- Source rewrite or fingerprint change -> artifact leaves skipped state and the
  focused regression must show refreshed parser/store visibility.
- Existing JSON without `skipped_files` -> serde default must load as `0`.

### 5. Good/Base/Bad Cases

- Good: a new monitor descriptor lists candidate roots and parser availability
  while leaving `SourceKind` unchanged.
- Base: Codex/Claude/OpenCode parser stats include processed, changed, skipped,
  emitted, and stored counts in CLI JSON/human output and TUI payloads.
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
- Human and subprocess tests covering pricing phase text, ordered additive
  NDJSON variants, stdout purity, and structured log phase fields.

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
