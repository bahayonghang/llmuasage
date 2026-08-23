# Implementation plan

## 0. Preflight and immutable baseline

- [x] Read `prd.md`, `design.md`, and `research/coverage-matrix.md`; confirm task status is `in_progress` before any
  product/test edit and preserve any newly appeared unrelated dirt.
- [x] Run the current locked/all-features test list and record: 14 integration targets, 202 integration leaf names,
  797 library unit tests and 999 total tests. Store normalized before-inventory evidence under this task.
- [x] Run the existing focused target list at least once with `--test-threads=1`; record any pre-existing failure
  separately rather than repairing outside scope.

## 1. Move-only test graph refactor

- [x] Add `autotests = false` and the eight explicit `[[test]]` entries from `design.md` to `Cargo.toml`; preserve the
  exact `architecture_dependencies` target name.
- [x] Create the domain `main.rs` module roots and move the 14 current integration files/architecture fixtures into
  the target tree. Split `m2_raw_archive_logs.rs`, `sync_regression.rs`, `report_commands.rs`, and
  `tui_panels_prop.rs` by the ownership map in `design.md` without changing test function bodies or names.
- [x] Extract only genuinely shared helpers into `tests/support/`; convert environment/process ownership to RAII
  without changing tested behavior. Keep source-specific encoders next to their source modules.
- [x] Run `cargo fmt`, compile/list every explicit target, and compare after-inventory with the before-inventory.
  Block unless every one of the 202 old leaf names appears exactly once and no old test disappeared.
- [x] Run each target independently:
  `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`, `sync`, and `tui`, all locked,
  all-features, single-threaded.

## 2. TDD vertical coverage slices

Complete each item as its own test -> red/sensitivity proof -> minimal green -> focused regression cycle. Do not write
all tests first.

- [x] **A1:** add `full_rebuild_preserves_parserless_rows_across_all_owned_tables` under
  `tests/sync/accounting.rs`; assert literal keys/counts for event, bucket, turn, tool call, cursor and source file.
- [x] **S2:** add `claude_recent_window_preserves_full_history_cursor_and_later_recovers_old_event` under the Claude
  source module.
- [x] **S3:** add `opencode_recent_window_preserves_high_waters_and_later_recovers_old_rows` using a real temporary
  OpenCode SQLite fixture.
- [x] **S4:** add `kimi_recent_window_preserves_full_history_cursor_and_later_recovers_old_event` using the real Kimi
  JSONL shape.
- [x] **S5:** extend the existing OMP recent-window test to assert no full-history cursor advancement and later
  unbounded recovery without orphan behavior rows or double counting.
- [x] **S6:** add `grok_recent_window_preserves_full_history_state_and_later_recovers_old_event` using the real Grok
  session/sidecar fixture.
- [x] **P1 unit red:** add table-driven behavior tests for Unix/Windows `file_path`, `path`, `cmd`, and shell
  `command` sentinels; assert sensitive values are absent while `input_fingerprint` remains stable.
- [x] **P1 integration red:** extend OMP sync-to-store coverage with exact raw path/command sentinels and assert none
  occur in persisted `safe_preview` while fingerprints remain present.
- [x] **P1 minimal green:** change only `src/parsers/behavior.rs::safe_tool_preview` sensitive-field handling. Preserve
  bounded `pattern`/`query`/`description`, classification, fingerprints, schema and public DTOs. Rerun both P1 tests
  and all behavior/parser/sync tests.

If A1 or S2-S6 is already green, perform one reversible source-local mutation to prove the new assertion fails,
restore it with an exact patch, and rerun green. Do not retain mutation code.

## 3. Active documentation references

- [x] Update current test paths/commands in source-sync, token-accounting, report-CLI and CI/toolchain code-specs.
- [x] Update live ADR references in ADR 0002, 0003 and 0013. Do not edit archived Trellis tasks.
- [x] Re-run `rg` across active specs, ADRs, README/developer docs, scripts, CI and `justfile` for the old flat paths;
  classify every remaining hit as active defect or intentional historical record.

## 4. Validation ladder

- [x] `cargo metadata --no-deps --format-version 1` shows exactly the eight intended explicit integration targets and
  the stable architecture target name.
- [x] Run focused tests for every new/extended case and the move-only identity reconciliation.
- [x] `cargo test --locked --all-features --test architecture_dependencies -- --test-threads=1`.
- [x] `cargo test --locked --all-features -- --test-threads=1`.
- [x] `python scripts/ci-rust.py`.
- [x] Dashboard Node tests and docs build remain covered by `just ci`; run `just ci` after the Rust gates.
- [x] `git diff --check` and inspect `git diff --stat` plus `git status --short` for unrelated changes, generated data,
  credentials, missing moves or forgotten root test files.
- [x] Record passed, failed, skipped and unverified evidence separately. Do not claim line/branch coverage.

## 5. Review and closeout gates

- [x] `trellis-check` verifies R1-R4 and AC1-AC9, with special attention to lost-test masking, environment restoration,
  direct-DB assertion justification, privacy-at-rest, exact parserless preservation, recent cursor/high-water behavior,
  and over-broad production fixes.
- [x] Update project code-spec only for durable test-layout commands/contracts learned during implementation; do not
  turn temporary file names into permanent rules.
- [ ] Commit/archive/journal only after all gates pass, following repository scope and Chinese/emoji commit policy;
  do not push or create a PR unless separately requested.

## Risky files and rollback points

| Surface | Risk | Rollback/checkpoint |
| --- | --- | --- |
| `Cargo.toml` test targets | Entire suites silently undiscovered | Exact 202-leaf move-only gate before new tests |
| Large file splits | Helpers/imports drift or tests duplicate | One target at a time; compile/list after each target |
| Env/process support | Parallel leakage or stuck children | RAII guard tests and single-target subprocess runs |
| Recent-window tests | Cursor/high-water accidentally advanced | Literal pre/post cursor assertions plus later full recovery |
| `safe_tool_preview` | Privacy fix removes unrelated analytics data | Limit omission to four sensitive fields; preserve fingerprint and non-sensitive previews |
| Active docs/spec paths | Stale developer commands | Final `rg` scan; archived task hits intentionally untouched |
