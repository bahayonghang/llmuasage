# Dashboard Query Modularization Evidence

## Evidence boundary

- Product behavior and SQL were not intentionally changed. The diff is limited
  to query module ownership, test-source partitioning, architecture guards, and
  the owning Trellis specification.
- Synthetic fixtures, public compile fixtures, focused Rust/Web/TUI tests, and
  architecture fixtures are **VERIFIED** below.
- A representative user database was not copied or inspected because this run
  did not have explicit authorization to access that private data. Current
  1d/7d/30d/all representative p95, payload, and RSS are therefore
  **UNVERIFIED**. Historical measurements cannot substitute for this gate.
- Follow-up 2026-08-25: authorization was granted later; the representative
  gate is now VERIFIED in `representative-performance.md`.

## Structure inventory

| File | Before production lines | After lines | Limit |
| --- | ---: | ---: | ---: |
| `src/query/mod.rs` | 4,198 | 215 | 1,200 |
| `overview.rs` | n/a | 618 | 1,000 |
| `breakdowns.rs` | n/a | 461 | 1,000 |
| `activity.rs` | n/a | 205 | 1,000 |
| `tools.rs` | n/a | 529 | 1,000 |
| `optimize.rs` | n/a | 629 | 1,000 |
| `comparison.rs` | n/a | 891 | 1,000 |
| `diagnostics.rs` | n/a | 613 | 1,000 |
| `snapshot.rs` | n/a | 264 | 1,000 |

`query/mod.rs` now owns declarations/re-exports, `Dashboard` construction, and
only shared scalar/filter helpers. Feature-local DTOs, SQL, private helpers, and
`impl Dashboard` methods moved to their vertical owners. Compatibility tests
remain in the logical `query::tests` module, while their source is partitioned
under `src/query/tests/` so fully-qualified test names do not change.

## Test discovery and architecture

- Before library leaf count: 811.
- Before sorted leaf SHA-256:
  `5dbb5e33509c352f48e2f4490442ef321151998d2fbabf3540128ef68d53ab82`.
- After excluding the one new ignored parity harness: the same 811 names and
  the same SHA-256. The parity harness is the only new library leaf.
- Architecture target grew from 6 to 10 tests. Positive/negative fixtures prove
  query-to-commands/web/tui rejection, canonical DTO/method ownership, duplicate
  implementation rejection, and file-size budgets.

## Synthetic statement and payload parity

Protocol: detached pre-move commit `c074ffd` and current tree, same fixed
`Fixture::seed_dashboard(180)`, one warm-up, five samples. Volatile
`generated_at` and temporary `archive_root` values are normalized before the
FNV-1a payload fingerprint. Seed/setup is outside timing.

| Shape | Before statements / bytes / hash | After statements / bytes / hash | Before p95 | After p95 |
| --- | --- | --- | ---: | ---: |
| full | 51 / 44,293 / `427b2233eb247475` | identical | 5.927 ms | 5.559 ms |
| core | 30 / 4,433 / `13e331b42385b3d4` | identical | 1.752 ms | 1.778 ms |
| interactive all | 27 / 4,284 / `b308b5956ff24b2` | identical | 1.534 ms | 1.478 ms |

Statement counts and normalized serialized bytes are exactly equal. Synthetic
wall times are diagnostic only; the representative 10% gate remains
`UNVERIFIED`.

## Verified commands

- `cargo test --locked --lib query::tests -- --test-threads=1`: 48 passed,
  3 ignored.
- `cargo test --locked --test api -- --test-threads=1`: 3 passed.
- `cargo test --locked --test architecture_dependencies -- --test-threads=1`:
  10 passed.
- `cargo test --locked --test query -- --test-threads=1`: 7 passed.
- `cargo test --locked --test cli reports:: -- --test-threads=1`: 13 passed.
- `cargo test --locked --test tui -- --test-threads=1`: 35 passed.
- `cargo test --locked --lib web::tests:: -- --test-threads=1`: 102 passed,
  1 ignored.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: passed.
- `python scripts/ci-rust.py`: passed; 800 library tests passed, 12 ignored,
  with every integration target and rustdoc passing.
- `just ci`: passed, including the Rust gate, Node syntax/unit tests, and
  VitePress production build.
