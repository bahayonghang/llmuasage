# Implementation Plan

1. Freeze behavior
   - [x] Add/identify parity fixtures for full, recent, rebuild, automatic repair, OMP migration, remote skip/sweep and cancellation.
   - [x] Capture human/NDJSON and `SyncSummary` baselines.
2. Extract application engine
   - [x] Move typed pipeline and helpers into `src/sync/engine.rs` without logic edits.
   - [x] Keep compatibility wrappers in commands and run focused parity tests.
3. Move default composition
   - [x] Introduce `DefaultSyncExecutor` and co-locate `JobRegistry::default` in sync.
   - [x] Switch Web/TUI/tests to sync-owned type; re-export old command path.
4. Strengthen architecture tests
   - [x] Add allowed-layer/impl-owner fixtures and mutation-sensitive negative cases.
5. Simplify commands
   - [x] Leave only CLI bootstrap, cancellation/reporters, formatting and wrappers.
   - [x] Remove stale boundary comments and update ADR/spec paths if ownership moved.
6. Validate
   - [x] Run sync/public API/architecture/Web/TUI focused tests and performance parity.
   - [x] Run fmt, strict clippy, serial tests, rustdoc, docs and `just ci`.

No task start or product edit is authorized until the planning summary is approved.
