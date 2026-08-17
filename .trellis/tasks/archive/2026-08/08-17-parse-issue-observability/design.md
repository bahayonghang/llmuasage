# Design: parse issue taxonomy

## Domain

Extend `ParseIssueKind` with `skipped` and `accounting_anomaly`.
Add matching counters on `ParseIssues` with serde defaults.
Keep the 4 MiB JSONL cap. Do not add a SQLite column.

`total()` stays malformed + oversized (faults). Add `informational_total()` for skipped + anomaly. Sync summary prints every non-zero class. Doctor warns only when `total() > 0`.

## Data flow

Parser writes ParseIssues into SourceSyncStats, then source_sync_status JSON.
CLI, doctor, source-status, and diagnostics consume the same JSON.
Dashboard and TUI receive four counters only, never samples.

## Reader

On an oversized record the reader exposes the 4 MiB prefix to the parser.
Accepted means recovered usage and no issue.
Skipped means skipped counter.
Malformed means malformed counter.
Anything else remains oversized.
Newline cursor, 4 MiB cap, and cancel-during-discard stay unchanged.

## Codex

Inspect the first 8 KiB of the prefix for token_count.

- other types become skipped
- token_count plus a complete JSON prefix becomes an event and no issue
- token_count that cannot be recovered stays oversized

## Other parsers

Zcode error/cancelled rows become skipped, not malformed.
Zcode cache overlap and computed_total mismatch become accounting_anomaly; events still store.
Antigravity open/decode/missing timestamp stay malformed.
Antigravity checksum and missing response_id with fallback key become accounting_anomaly.
Grok sidecar over the size cap stays oversized; bad sidecar JSON stays malformed.
OpenCode is unchanged.

## Surfaces

sync summary prints non-zero classes. Faults use warning color; skipped and anomaly do not.
CLI samples: kind, offset, basename from source_cursor.file_path. No full path, no record text.
doctor warns only on malformed+oversized.
source-status appends a one-line counter summary.
SyncSourcePayload adds four u64 fields with serde defaults.
Dashboard source cards and TUI wide table show non-zero counts.
Copy keys live in web copy.js / TUI English strings; no frontend string-splitting of reasons.

## Compatibility

Old parse_issues_json missing new fields deserializes as zero.
Unknown kind in samples fails like invalid JSON today.
Update source-sync-contracts.md in the same change.
No schema migration unless serde compat proves insufficient.
