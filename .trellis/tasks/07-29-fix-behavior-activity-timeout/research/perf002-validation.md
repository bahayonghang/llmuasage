# PERF-002 supervisor validation

## Red-capable loops

- Rust: `cargo test --all-features web::tests::dashboard_hard_timeout_supervises_non_cooperative_work -- --exact`
  initially failed because `WebState` had no `dashboard_query_supervisor`.
- Python: `python -B .trellis/tasks/07-29-fix-behavior-activity-timeout/research/test_profile_activity_first_touch.py`
  initially failed because the harness neither parsed query IDs nor waited for a
  matching orphan-settled event before the warm request.

## Implemented contract

- A dashboard timeout still returns at the configured hard deadline and never
  awaits blocking work in the request future.
- The blocking closure retains its semaphore permit and inflight guard until it
  actually exits. The supervisor owns and awaits the timed-out JoinHandle in a
  background Tokio task.
- Timeout and settled events share a process-local query ID. Settled logs expose
  only section, orphan duration, and a bounded join-outcome label.
- `/api/diagnostics` appends live inflight, timeout, orphan, and last-duration
  fields after the cached filesystem diagnostics payload is loaded.
- A timed-out/cancelled first-touch request cannot be followed by its warm pair
  until the harness observes the matching settled event.

## Review findings fixed

- Quoted tracing fields such as `section="activity"` now match the settled parser.
- The non-cooperative Rust regression uses start/release channels, so it proves
  the closure is running instead of assuming it starts within a short timer.
- A wrong query ID or missing settled event hard-fails; query IDs are removed
  before sanitized evidence is written.

## Verification

- Focused Rust supervisor, SQLite interruption, diagnostics cache, and Behavior
  deadline tests: passed.
- `cargo test --all-features web::tests -- --test-threads=1`: 90 passed, 1
  pre-existing measurement test ignored.
- Python harness tests: 27 passed.
- Ruff check/format and Pyright: passed with zero diagnostics. Ruff could not
  update its optional workspace cache because of local permissions; lint still
  exited successfully.
- `python scripts/ci-rust.py`: passed, including fmt, Clippy with warnings denied,
  569 library tests, integration tests, and rustdoc.
- `git diff --check`: passed; only pre-existing workspace line-ending warnings
  were emitted outside this task's files.

No snapshot prepare, reboot, D1/D2 query optimization, migration, commit, archive,
journal, or push was performed during this gate.
