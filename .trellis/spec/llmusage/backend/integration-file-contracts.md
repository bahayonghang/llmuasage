# Integration File Contracts

## 1. Scope / Trigger

Apply this contract only when `llmusage uninstall` cleans up hook/plugin
artifacts written by older releases. Current releases do not install or probe
third-party integrations.

## 2. Signatures

- `cleanup_all(app, store)` dispatches legacy cleanup for Codex, Claude,
  OpenCode, and Antigravity/legacy Gemini artifacts.
- `write_file_atomic_and_record(path, contents, record)` replaces an external
  config during cleanup and records the actual action.
- `remove_file_atomic_and_record(path, record)` removes an owned legacy plugin
  and records the actual action.
- `recover_and_cleanup_residue(path)` restores interrupted work first, then
  removes exact sibling temp/pending/recovery residue.

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
- Claude and Antigravity/legacy Gemini cleanup identifies owned command entries
  by the stable `llmusage-hook` marker, not an exact current-version command
  string. Historical quote variants must be removed while sibling user hooks
  remain structurally unchanged.
- Codex restores `codex_notify_original.json` when present and consumes that
  exact marker only after a successful restore. Without the marker, remove
  `notify` only when the current command is llmusage-owned; preserve every
  other notify value.
- OpenCode removes `llmusage-tracker.js` only when its contents include
  `LLMUSAGE_LOCAL_PLUGIN`. An unowned file is preserved even when sibling
  residue is cleaned.
- Cleanup owns only exact wrappers and sibling atomic residue. Never scan or
  bulk-delete `backups/*.bak`; historical integration backups and
  `llmusage.db.pre-0.5.0` remain available.
- `integration_install` records only an actual cleanup change or a cleanup
  failure. A no-op must not modify third-party config, create a backup, or
  write an integration action row. A second successful cleanup is a no-op.
- Shared legacy wrapper files use the explicit `legacy_hook_wrappers` audit key
  rather than being misattributed to one source. Wrapper removal uses the same
  remove-and-record compensation path as other owned files: an action-record
  failure restores the wrapper.
- One integration cleanup failure must not prevent later integrations or shared
  wrappers from being attempted. Aggregate every failure and return an error
  after the complete cleanup pass.

## 4. Required Tests

- Inject failures at write, flush, replace, and action-record stages. After
  each failure, the target is complete old or complete new, never missing or
  truncated.
- On Windows, exercise both existing-target and missing-target paths.
- Exercise interrupted-operation recovery on the next attempt.
- Exercise cleanup for Claude, Codex, OpenCode, Antigravity, and legacy Gemini
  using temporary roots, preserving unrelated user entries and unowned files.
- Cover both historical Windows quote variants, legacy `--source gemini`,
  Codex marker consumption, OpenCode ownership marking, and exact wrapper
  cleanup.
- Cover wrapper-only cleanup, its `legacy_hook_wrappers` audit row, action-record
  compensation, and the following no-op cleanup.
- Make one integration cleanup fail and assert later integrations and wrappers
  are still cleaned and audited.
- Cover pending/recovery/temp residue when the target is absent and when an
  unowned target remains; each actual residue cleanup must be audited.
- Run cleanup twice and assert the second run changes no config, creates no
  backup, and writes no `integration_install` row.
- Assert historical `*.bak` and database upgrade backups remain untouched.
- Assert sibling temp, pending, and recovery files do not remain after success
  or successful compensation.
