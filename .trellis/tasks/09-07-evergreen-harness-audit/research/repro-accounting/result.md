# Automatic token-accounting repair data-loss reproduction

Command (repository root, Rust 1.97.0, offline dependency resolution):

```powershell
cargo run --manifest-path .trellis/tasks/09-07-evergreen-harness-audit/research/repro-accounting/Cargo.toml --target-dir target --offline
```

Result: `PASS` for reproducing the suspected defect. The process exited 0 after
confirming both failure paths. The probe uses only temporary homes below this
directory and does not modify repository production code or tests.

```text
parser_failure.error=no such table: message: Error code 1: SQL logic error
parser_failure.before=usage_event:1,usage_bucket_30m:1,usage_turn:1,usage_tool_call:0,source_cursor:1,source_sync_status:1,source_file:0
parser_failure.after=usage_event:0,usage_bucket_30m:0,usage_turn:0,usage_tool_call:0,source_cursor:0,source_sync_status:0,source_file:0
parser_failure.marker_after=None
cancellation.before=usage_event:1,usage_bucket_30m:1,usage_turn:1,usage_tool_call:0,source_cursor:1,source_sync_status:1,source_file:1
cancellation.after=usage_event:0,usage_bucket_30m:0,usage_turn:0,usage_tool_call:0,source_cursor:0,source_sync_status:0,source_file:0
cancellation.marker_after=None
```

The parser-failure branch first performs one successful OpenCode sync, clears
the accounting marker, then drops the source `message` table. The ordinary
sync detects legacy accounting, deletes the live source rows, and only then
surfaces the parser error. The cancellation branch performs one successful
Codex sync, clears the marker, and cancels when
`TokenAccountingRepairStarted` is received. It likewise returns with every
live source row deleted.

Production anchors:

- `src/sync/engine.rs:228-265`: automatic repair emits Started and calls the
  destructive reset before parser execution.
- `src/sync/engine.rs:289-313`: parser/store errors return without restoring
  the old source rows.
- `src/store/schema.rs:298-318,393-433`: the reset deletes the source's live
  facts, aggregates, cursors, status, and file state.
- `tests/sync/accounting.rs:690-730,737-790,916-949`: current failure and
  cancellation tests check the marker/events but do not assert that old rows
  survive.

Recommended first repair: stop automatic destructive repair and preserve the
legacy source rows, skip that source for the current ordinary sync, and return
or surface an actionable explicit-rebuild requirement. This is the smallest
complete safety fix and changes the documented automatic-repair behavior.
Keep explicit `sync --rebuild` destructive semantics because that command is a
direct user request. If automatic repair must remain, design a source-level
staging path: parse and validate a complete replacement without touching live
rows, then delete/promote event, bucket, behavior, cursor, status, and
source-file state in one transaction. Do not hold an SQLite write transaction
open across asynchronous filesystem parsing.

