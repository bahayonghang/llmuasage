# Integration File Contracts

## 1. Scope / Trigger

Apply this contract when installing, updating, cleaning up, or uninstalling
Claude, Codex, OpenCode, Antigravity, or another third-party integration file.

## 2. Signatures

- `write_file_atomic(path, contents)` for llmusage-owned files that do not have
  an integration action row.
- `write_file_atomic_and_record(path, contents, record)` for external config
  replacement followed by `record_action`.
- `remove_file_atomic_and_record(path, record)` for external plugin removal
  followed by `record_action`.

## 3. Contracts

- Temp, pending, and recovery files are siblings of the target. Temp and
  recovery creation uses `create_new`; successful and compensated operations
  remove them.
- Write the full temp payload, flush it with `sync_all`, preserve existing
  target permissions, then replace the target atomically.
- On Windows, replace an existing target with `ReplaceFileW` and
  `REPLACEFILE_WRITE_THROUGH`. Never remove an existing target before replace.
  A missing target uses same-directory rename. Rust does not provide the
  directory handle required for an additional parent-directory fsync on
  Windows, so `WRITE_THROUGH` is the documented durability boundary.
- On Unix, same-directory rename replaces the target and the parent directory
  is synced after namespace changes.
- Before an external change, persist a sibling recovery snapshot and pending
  marker. A later operation must restore an interrupted pending change before
  starting a new one.
- If `record_action` fails, restore the prior bytes or prior absence. If that
  recovery also fails, retain the pending marker and recovery path and return a
  typed error that identifies both failures; never report success.
- Integration tests must redirect all home/config roots to temporary
  directories. Tests must never install into real user configuration.

## 4. Required Tests

- Inject failures at write, flush, replace, and action-record stages. After
  each failure, the target is complete old or complete new, never missing or
  truncated.
- On Windows, exercise both existing-target and missing-target paths.
- Exercise interrupted-operation recovery on the next attempt.
- Exercise install/uninstall for Claude, Codex, OpenCode, and Antigravity using
  temporary roots, preserving unrelated user entries.
- Assert sibling temp, pending, and recovery files do not remain after success
  or successful compensation.
