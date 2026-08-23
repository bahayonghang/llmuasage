# Trellis check evidence

## Discovery and conservation

- `cargo metadata --no-deps --format-version 1` reports exactly eight explicit integration targets:
  `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`, `sync`, and `tui`.
- Final discovery is 799 library tests plus 208 integration tests, for 1007 total.
- The final integration target counts are 3 / 3 / 30 / 7 / 6 / 2 / 122 / 35 in the order above.
- Multiset comparison against `baseline-test-inventory.md` found all 202 old integration leaf names exactly once,
  with zero missing and zero duplicates. The six additional integration leaves are A1, S2, S3, S4, S6, and P1;
  S5 extends the existing OMP leaf.
- No Rust test file remains at the `tests/` root, and no integration test is ignored.

## Findings fixed during check

- Replaced manual process-environment restoration with the shared `tests/support/env.rs` RAII guard. The guard
  serializes environment fixtures within each integration target, preserves non-Unicode values, restores in reverse
  order, survives constructor errors and panic unwinding, and makes explicit restore plus `Drop` idempotent.
- Added the genuinely cross-target `tests/support/process.rs` helper. Every `CARGO_BIN_EXE_llmusage` subprocess now
  verifies the binary path first, and fallible spawns attach operation context.
- Strengthened P1 so sensitive-only inputs must persist an empty `safe_preview`, not merely avoid the full sentinel.
  Added direct coverage that Bash test-command classification remains `Testing` after command-preview redaction.
- Reconciled active code-spec, ADR, PRD and coverage-matrix references with the domain test paths. Archived task
  history was not edited.

## Verification

- `cargo fmt --all -- --check`: pass.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: pass.
- Focused behavior classification unit test: 1 passed.
- `api`: 3 passed; `cli`: 30 passed; `remote`: 6 passed; `sync`: 122 passed.
- `architecture_dependencies`: 3 passed under the stable target name.
- `cargo test --locked --all-features -- --test-threads=1`: 999 passed, 8 ignored, 0 failed. The eight ignored
  tests are pre-existing opt-in local/performance measurements in library modules; none is an integration test.
- `python scripts/ci-rust.py`: pass, including rustdoc.
- `just ci`: pass, including CI-contract checks, dashboard JavaScript tests and the VitePress build.
- `git diff --check`: pass; task/test trailing-whitespace scan: zero; stale active flat-test paths: zero.
- `task.py validate`: pass with six implementation entries and five check entries.

## Unverified or intentionally deferred

- No line/branch coverage percentage is claimed; the task deliberately uses the contract/risk matrix instead.
- The final tree contains no durable transcript of the temporary mutation sensitivity runs required by the execution
  plan for already-correct A1/S2-S6 behavior. Current behavior and assertions are green, but the historical red step
  is `UNVERIFIED` and cannot be reconstructed honestly during this review.
- Eight opt-in local/performance measurement tests remain skipped by the ordinary gate, as designed.
