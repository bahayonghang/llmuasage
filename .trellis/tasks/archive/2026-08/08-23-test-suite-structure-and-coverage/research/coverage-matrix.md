# Integration test baseline and risk matrix

## Baseline

- `cargo test --locked --all-features -- --list` currently discovers 797 library unit tests and
  202 integration tests under `tests/`, for 999 total tests.
- The 202 integration tests are spread across 14 root targets:

| Current target | Tests | Primary responsibility today |
| --- | ---: | --- |
| `architecture_dependencies` | 3 | AST dependency boundaries and fixtures |
| `hour_of_week` | 2 | query timezone/DST behavior |
| `local_flow` | 6 | init/sync/export/uninstall and pricing flows |
| `logs_session_analytics` | 1 | logs/session detail query |
| `m2_raw_archive_logs` | 15 | raw archive, recent sync, reset, jobs, cancellation, subprocess output |
| `public_api` | 3 | root facade and CLI validation codes |
| `remote_lifecycle` | 5 | unreachable host and remote status lifecycle |
| `remote_shard_transport` | 1 | read-only shard emission |
| `report_commands` | 24 | reports plus logs, diagnostics, doctor, catalog, statusline |
| `source_file_state` | 2 | source-file state transitions |
| `sync_regression` | 84 | generic sync plus ten source families, migrations, locks, pricing and run-log |
| `token_accounting_parity` | 17 | accounting repair/rebuild safety |
| `tui_panels_prop` | 35 | all interactive panels and property tests |
| `web_sessions_endpoint` | 4 | `Dashboard::top_sessions` query behavior, not a real HTTP endpoint |

The move-only checkpoint must preserve the complete set of 202 existing test function leaf names.
Module prefixes and target names may change; a missing, duplicated, or silently undiscovered leaf name blocks the
coverage phase.

## Agreed test seams

| Seam | How behavior is observed | Direct SQLite allowed? |
| --- | --- | --- |
| CLI | `CARGO_BIN_EXE_llmusage` subprocess status/stdout/stderr | Only fixture seeding before the subprocess |
| Sync/source | `commands::sync::run_once_with_options` and public `Store`/`Dashboard` postconditions | Only for persistence invariants with no public reader, such as cursor rows and privacy-at-rest |
| Query | Public `Dashboard` query methods | Fixture setup only |
| Remote | Public remote command/import APIs and emitted events | Fixture setup and exact persisted ownership checks |
| TUI | `ratatui::TestBackend` buffers and public controller state | No |
| Architecture/layout | Cargo target discovery and `syn`-based source inspection | Not applicable |

Mocks remain limited to external process/SSH boundaries. Source fixtures use isolated temporary roots and real
SQLite stores; tests do not mock internal parser/store/query collaborators.

## Contract coverage matrix

| ID | Contract/risk | Current evidence | Decision |
| --- | --- | --- | --- |
| L1 | Test discovery survives folder refactor | `Cargo.toml` declares eight explicit domain targets; the recorded before/after comparison preserved all 202 old leaf names with no duplicates | Verified; six new integration tests are additive |
| S1 | Codex recent window filters by event time, does not advance full-history cursor, later full sync recovers history | `tests/sync/runtime/recent.rs` covers the real Codex parser end to end | Preserved; no duplicate test |
| S2 | Claude recent-window behavior | `tests/sync/sources/codex_claude.rs` covers bounded filtering, empty full-history cursor, and later recovery | Added one vertical integration slice |
| S3 | OpenCode recent SQL lower bound and high-water immutability | `tests/sync/sources/opencode.rs` covers a real SQLite fixture, all three bounded high-waters, later recovery and idempotency | Added one vertical integration slice |
| S4 | Kimi recent-window behavior | `tests/sync/sources/kimi.rs` covers the real JSONL shape, bounded filtering, cursor immutability and later recovery | Added one vertical integration slice |
| S5 | Pi/Oh My Pi recent-window behavior and behavior-row alignment | `tests/sync/sources/pi_omp.rs` proves bounded filtering, no cursor advancement, no orphan behavior rows, later recovery and idempotency | Extended the existing OMP integration slice |
| S6 | Grok recent-window behavior | `tests/sync/sources/grok.rs` covers a real sidecar session, bounded filtering, state immutability, later recovery and idempotency | Added one vertical integration slice |
| S7 | Antigravity, ZCode and DSH recent-window behavior | Preserved in `tests/sync/sources/antigravity.rs`, `zcode.rs`, and `deepseek_harness.rs` | Preserved; no duplicate tests |
| A1 | Successful full rebuild preserves parserless event/bucket/behavior/cursor/source-file rows | `tests/sync/accounting.rs` seeds and asserts literal rows in all six owned tables after a successful parser-backed rebuild | Added one vertical integration slice |
| P1 | Behavior evidence does not persist raw paths/commands | `src/parsers/behavior.rs` has table-driven unit coverage; `tests/sync/sources/pi_omp.rs` verifies empty persisted previews and non-empty fingerprints for four sensitive fields | Added unit + sync-to-store regression coverage and the authorized redaction fix |
| V1 | CLI/Web/JobRegistry reject the same invalid sync inputs and create no jobs | CLI table at `tests/api/facade.rs`, JobRegistry table at `src/sync/job_registry.rs`, and Web invalid-input tests at `src/web/mod.rs` | Preserved; matrix records existing cross-surface evidence, no redundant test |
| W1 | Public router/payload excludes sensitive local data | Real TCP and recursive forbidden-key/value assertions at `src/web/mod.rs:2347-2479` | Preserve inline tests; do not move private web tests into `tests/` |
| T1 | TUI duplicate sync action cancels and bounded shutdown restores state | Multi-thread controller test in `src/tui/sync_control.rs:300-350` | Preserve inline test; reorganize only external panel tests |
| Q1 | Report/query JSON, filters and timezones | 24 report command integrations, Top Sessions integrations, hour/DST tests and extensive query unit tests | Reorganize by seam; add no quantity-driven duplicates |

## Active path-reference reconciliation

- Source-sync, token-accounting and report-CLI code-specs now name the domain test paths.
- ADR 0001, 0002, 0003 and 0013 now name the moved CLI or source test modules.
- `.trellis/spec/llmusage/backend/ci-toolchain-contracts.md` and CI continue to use the stable
  `architecture_dependencies` target name.
- Archived Trellis task artifacts remain unchanged as historical evidence.

## Deferred evidence

- No line or branch coverage percentage is claimed because `cargo-llvm-cov` is not installed and the user chose
  contract/risk coverage instead.
- Mutation sensitivity is required for new tests that protect already-correct behavior: apply one small,
  task-owned reversible mutation, observe the focused test fail, restore with an exact patch, then observe green.
  The privacy test needs no synthetic mutation because current production behavior supplies a real red state.

## Final discovery

- Final library tests: 799; final integration tests: 208; final total: 1007.
- All 202 baseline integration leaf names remain exactly once; six new integration leaves and two new library unit
  tests are additive.
- The ordinary all-features gate passes 999 tests and intentionally skips eight pre-existing opt-in
  local/performance measurements. No integration test is ignored.
- Full check evidence is recorded in `research/check-evidence.md`.
