# Test suite structure and coverage design

## 1. Design objective

Replace the flat integration-test directory with explicit, domain-addressable Cargo test targets while preserving
every existing integration test, then add only the high-risk contract cases confirmed in
`research/coverage-matrix.md`. Structural movement and coverage additions stay in one task because both mutate the
same Cargo test graph, fixture ownership and validation baseline; splitting them into parent/children would require
the coverage child to depend on an unstable intermediate test tree and duplicate the 202-test reconciliation gate.

## 2. Target directory and Cargo contract

Set `autotests = false` in `[package]` and declare these explicit targets in `Cargo.toml`:

| Cargo target | Path | Compatibility |
| --- | --- | --- |
| `api` | `tests/api/main.rs` | New focused target for root-facade tests |
| `architecture_dependencies` | `tests/architecture/main.rs` | Name remains unchanged for CI |
| `cli` | `tests/cli/main.rs` | Reports, lifecycle commands and subprocess contracts |
| `query` | `tests/query/main.rs` | Dashboard queries, sessions, logs and timezone behavior |
| `remote` | `tests/remote/main.rs` | Host lifecycle and shard transport |
| `store` | `tests/store/main.rs` | Raw archive, source-file state and reset persistence |
| `sync` | `tests/sync/main.rs` | Sync orchestration, source lifecycles, jobs and accounting |
| `tui` | `tests/tui/main.rs` | External TestBackend panel behavior |

The intended tree is:

```text
tests/
  api/
    main.rs
    facade.rs
  architecture/
    main.rs
    fixtures/*.rs
  cli/
    main.rs
    local_flow.rs
    reports.rs
    operations.rs
  query/
    main.rs
    hour_of_week.rs
    logs.rs
    top_sessions.rs
  remote/
    main.rs
    lifecycle.rs
    shard_transport.rs
  store/
    main.rs
    raw_archive.rs
    reset.rs
    source_file_state.rs
  sync/
    main.rs
    accounting.rs
    jobs.rs
    lifecycle.rs
    progress_io.rs
    sources/
      mod.rs
      codex_claude.rs
      opencode.rs
      kimi.rs
      pi_omp.rs
      grok.rs
      antigravity.rs
      zcode.rs
      deepseek_harness.rs
  tui/
    main.rs
    shell.rs
    panels/
      mod.rs
      overview.rs
      period.rs
      usage.rs
      stats.rs
      behavior.rs
      models.rs
  support/
    mod.rs
    env.rs
    process.rs
    store.rs
```

Exact helper filenames may shrink when an existing helper has only one owner. Domain-specific source encoders and
seeders stay beside their source tests; `tests/support/` is limited to helpers reused by at least two domain modules.
No tests live in support modules, because compiling support into multiple integration crates would duplicate them.

## 3. Existing-test conservation

Structural migration is a separate checkpoint from new coverage:

1. Capture the 14 current targets, 202 integration leaf names and 999 total test count.
2. Move/split files and wire explicit targets without adding, deleting or renaming test functions.
3. Compare normalized leaf-name sets. Module prefixes and target names may change, but all 202 old leaf names must
   appear exactly once.
4. Run all eight explicit targets with `--test-threads=1` and the unchanged
   `architecture_dependencies` focused CI command.
5. Only after this gate is green may the first new test be added.

This ordering prevents newly added tests from hiding tests silently lost during a folder move.

## 4. Fixture ownership and isolation

- Pure query/store tests use a temporary `AppPaths` root and real SQLite database.
- Environment-mutating source fixtures use an RAII `ScopedEnv` helper that owns a process-local mutex, snapshots
  every changed variable and restores it in reverse order on success, error or panic.
- Subprocess helpers assert `CARGO_BIN_EXE_llmusage` exists and attach command/stdout/stderr context.
- Child processes and bound servers have Drop/shutdown guards; no test reads the real user home, active database or
  external network.
- The feature-gated `llmusage::testing` module is not made a new dependency of default integration tests, because
  integration crates compile the library without `cfg(test)` unless `testing` is enabled. Reuse it only in
  all-features-only tests that already opt into that contract; otherwise keep helpers under `tests/support/`.

## 5. TDD seams and vertical slices

Tests use the agreed seams in `research/coverage-matrix.md`. The coverage phase proceeds one slice at a time:

### Slice A1 — parserless rows survive a successful full rebuild

- Seam: `commands::sync::run_once_with_options` plus literal Store persistence postconditions.
- Fixture: seed a rebuildable parser source and parserless Antigravity rows in `usage_event`,
  `usage_bucket_30m`, `usage_turn`, `usage_tool_call`, `source_cursor`, and `source_file`.
- Assertion: successful full rebuild changes the parser source as expected and preserves exact parserless counts and
  keys in all six tables.
- Production change: none expected. If it fails, only a directly implicated reset-boundary correction is permitted;
  schema or rebuild-policy changes return to planning.

### Slices S2-S6 — real-source recent windows

- Seams: one real source fixture -> `run_once_with_options(recent_days=...)` -> Store/Dashboard state -> later
  unbounded sync.
- Sources: Claude, OpenCode, Kimi, Grok, and the existing OMP slice.
- Shared invariant: old and recent records coexist in one source artifact/database; bounded sync imports only recent
  records, does not advance/reset the source's full-history cursor/high-water, and a later full sync recovers the old
  record without double counting.
- Expected production change: none. Any failure is handled as its own red-green slice and may receive only the
  smallest source-local correction needed by the documented contract.

### Slice P1 — sensitive tool values never persist in `safe_preview`

- Unit seam: behavior evidence returned by the parser behavior module.
- Integration seam: OMP file -> normal sync -> persistence-at-rest query, which is allowed because no public API
  exposes the raw stored preview contract.
- Red assertions: exact sentinel path, Windows path, Unix path and shell command do not occur in `safe_preview`;
  `input_fingerprint` remains non-empty and stable.
- Minimal green mechanism: `safe_tool_preview` omits raw values for `file_path`, `path`, `cmd`, and `command`.
  Existing bounded `pattern`, `query`, and `description` previews remain unchanged; tool classification, tool name,
  fingerprinting, event keys, schema and DTOs do not change.

For a new test that is green against existing code, demonstrate sensitivity with one temporary, narrowly scoped
mutation, run only that focused test to red, restore through an exact patch, then rerun green. Do not retain mutation
helpers or build a mutation framework.

## 6. Documentation and compatibility

- Update active code-spec and ADR references from old flat paths to the new domain modules.
- Keep archived `.trellis/tasks/archive/**` paths unchanged.
- Preserve the `architecture_dependencies` target name and update `.github/workflows/ci.yml` only if its path or
  command actually requires it; the planned explicit target keeps the current command valid.
- No CLI/API/SQLite/schema/serialization compatibility change is introduced. The only authorized observable
  production adjustment is removal of raw path/command values from newly generated `safe_preview` rows; existing
  historical rows are not migrated.

## 7. Failure handling and rollback

- If the 202-test identity gate fails, stop before adding coverage and repair only module wiring/moves.
- If a new test exposes a schema, API or unrelated product defect, return to planning rather than broadening the fix.
- Moves are tracked Git changes. Reversal uses explicit, verified paths or exact patches; never use destructive Git
  reset/checkout and never touch unrelated worktree changes.
- No user data migration or cleanup exists. Temporary files, databases, ports and child processes are task-owned and
  removed by their guards.
