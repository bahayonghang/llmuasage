# Sync application boundary parity and performance evidence

## Scope

- Date: 2026-08-24
- Before commit: `c5802fe`
- Change type: ownership/module extraction only; parser, Store, SyncShard, remote wire format, and public request/event/summary shapes are unchanged.
- Data boundary: deterministic repository fixtures only. No user database, source path, host credential, or event content was inspected.

## Core equivalence

The complete block from `RemoteRunOutcome` through rebuild/repair/remote helpers was compared against `c5802fe:src/commands/sync.rs`. After normalizing only the two visibility changes required by the new sibling modules, both 21,685-character blocks have the same SHA-256:

```text
0249be9becf1476fe25e1048ad1cf467de9c7794bf9e1f301e07d67b84262272
```

This byte equivalence covers parser selection, SQL calls, rebuild/repair branches, remote import/sweep, event sends, status writes, and `SyncSummary` assembly. The `run_tracked` lifecycle was moved into the engine with the same start/success/failure statements; compatibility wrappers contain delegation only.

## Synthetic hot-sync timing

Exact test:

```powershell
cargo test --quiet --locked --all-features --test sync sources::codex_claude::sync_hot_run_and_append_remain_incremental -- --exact --test-threads=1
```

The baseline was built from a temporary `git archive c5802fe`; baseline and current runs were interleaved to reduce order bias. Each test creates isolated synthetic Codex/Claude/OpenCode inputs and asserts hot-run idempotency, append recovery, source status, and a final event count of five.

| Build | wall ms (9 runs) | p50 ms | observed max ms |
| --- | --- | ---: | ---: |
| `c5802fe` | 1339.3, 1354.3, 1354.1, 1379.3, 1330.5, 1404.2, 1431.3, 1365.3, 1527.2 | 1365.3 | 1527.2 |
| current | 1355.8, 1337.8, 1312.6, 1342.7, 1307.9, 1379.7, 1381.8, 1395.2, 1497.2 | 1355.8 | 1497.2 |

- p50 delta: `-0.7%`.
- observed-max delta (the empirical p95 rank for nine samples): `-2.0%`.
- event and status assertions passed in every baseline and current run.
- SQL/event statement ownership is byte-equivalent as recorded above; no statement-producing branch was added or removed.

## Focused validation

- sync integration: 122 passed.
- API facade: 3 passed.
- remote integration: 6 passed.
- TUI integration: 35 passed.
- architecture dependencies: 6 passed, including three regression classes.
- strict all-target/all-feature clippy: passed.

Representative user-copy hot/cold p95 remains **UNVERIFIED** because no authorization to copy or inspect a real usage database was granted.
