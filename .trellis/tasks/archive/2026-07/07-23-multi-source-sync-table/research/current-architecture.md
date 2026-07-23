# Current Architecture And Screenshot Diagnosis

Evidence date: 2026-07-23

## Existing Source Boundary

- `src/domain/models.rs:11-39` owns the persisted `SourceKind` identifiers. A new parser-backed source therefore needs a stable id; it must not be represented only by a monitor label.
- `src/domain/source_descriptor.rs:91-205` owns activation mode, parser/integration capabilities, token quality, privacy class, and aliases. This is the correct place to distinguish parser-backed, total-only, and monitor-only states.
- `src/registry.rs:23-28` returns the fixed `Vec<Box<dyn SourceParser>>`. Registering a parser here also makes it part of rebuild/token-accounting source boundaries.
- `src/parsers/source_parser.rs:20-41` requires one async parse/commit implementation returning `SourceSyncStats` and sharing `SyncRunWriter`.
- `src/store/mod.rs:46-76` provides `FileCursor` with fingerprint, size, tail signature, byte offset, cumulative token snapshot, and last model. This is sufficient for Kimi/Pi JSONL state without a migration.
- `src/parsers/file_state.rs:29-90` decides append versus full reparse from the cursor and signatures. New file readers should use this state machine and test truncation/rotation rather than inventing a parallel cursor.
- `src/parsers/driver.rs:92-144` emits `SourceFinished` after the parser and missing-file sweep. The event is a lifecycle/progress signal, not a second summary protocol.

## Sync Output Diagnosis

- `src/commands/sync_progress.rs:442-539` maps lifecycle events to human progress text. The `SourceFinished` arm at `:532-539` creates a permanent per-source completion line.
- `src/commands/sync_progress.rs:253-263` makes `SourceFinished` permanent in the TTY bar renderer; the line renderer also prints it, so both modes duplicate the final summary.
- `src/commands/sync.rs:283-287` prints the final summary on stdout after the reporter completes.
- `src/commands/sync_summary.rs:12-68` already has a pure aligned formatter with the desired per-source metrics. `:101-134` builds rows from `SourceSyncStats`; the separate `- totals` line is not a table row.

## Planned Boundary

Keep `SyncEvent` and `SourceSyncStats` wire shapes unchanged. Change only the human presentation contract: `SourceFinished` closes/refreshes progress without a permanent success sentence, and `format_summary_lines` emits a `TOTAL` row. Preserve error and cancellation lines because they are diagnostics, not duplicate success summaries.

## Cross-Layer Constraints

The source-sync contract requires parser stats, source status, TUI/dashboard payloads, and human output to remain compatible. The token-accounting contract requires authoritative upstream totals to win, cache channels to remain separate, and reasoning to stay diagnostic unless the source proves it is disjoint from output. These constraints apply to Kimi and Pi/OMP.
