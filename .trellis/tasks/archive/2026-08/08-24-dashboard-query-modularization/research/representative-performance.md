# Representative Interactive Performance Evidence (A6/R8)

## Protocol

- Date: 2026-08-25, after user authorization for a read-only temp copy of the
  live database.
- Database: consistent snapshot of `~/.llmusage/llmusage.db` taken via the
  SQLite backup API into a temp `LLMUSAGE_HOME`; 1.16 GB on disk. The live
  database was never opened by the benchmark process. The copy was deleted
  after the run and the server port was confirmed released.
- Binary: release build of HEAD `5c6fddc` (`refactor(query): 拆分 Dashboard
  查询垂直模块`), the exact tree that passed `just ci`.
- Server: `llmusage serve --port 39417 --no-open`, loopback only.
- Harness: `scripts/benchmark-dashboard-range.mjs --iterations 5`; one warm-up
  plus five sequential HTTP samples per range against
  `/api/dashboard?scope=interactive`; p95 by nearest-rank. Raw allowlisted
  output in `representative-interactive-harness.json`.

## HTTP interactive matrix

| Range | p95 (ms) | max payload (bytes) | ≤400 ms | ≤128 KiB |
| --- | ---: | ---: | --- | --- |
| 1d | 81.21 | 16,057 | PASS | PASS |
| 7d | 79.72 | 23,941 | PASS | PASS |
| 30d | 80.07 | 34,049 | PASS | PASS |
| all | 91.20 | 74,805 | PASS | PASS |

## Browser spot checks (same copy)

- Range-preset click feedback p95 budget 100 ms: observed 4.5–12.6 ms.
- Critical render: 59.4–130.7 ms; rapid preset switching completed with
  latest-wins semantics; no long tasks recorded.

## Gate interpretation

- Absolute budgets R8 (≤400 ms / ≤128 KiB): all four ranges pass with margin.
- Wall-time vs historical baseline (`07-11-dashboard-time-range-performance`,
  representative p95 142–239 ms): current p95 80–91 ms is below the historical
  band; no regression.
- Payload vs historical band (15–66 KiB): all-range max is 73 KiB. This is not
  attributed to the refactor: synthetic parity already proved byte-identical
  serialized payloads and statement counts pre/post move on a fixed seed, so
  the representative delta reflects one month of additional data, not module
  ownership changes.

## Still UNVERIFIED

- Server RSS sampling and cold OS file cache were not exercised by this
  harness; they remain `UNVERIFIED` per the evidence model and are not claimed
  as PASS.
