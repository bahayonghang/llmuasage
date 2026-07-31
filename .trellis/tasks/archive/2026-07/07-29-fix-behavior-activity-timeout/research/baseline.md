# Activity Step 1 baseline

## Gate result

**NO-GO for D1/D2 implementation.** This run did not reproduce the Step 1 gate
signature (cold `all` above 3 seconds with warm requests meeting the budget).
Every copy-backed-cold `all` request completed in 1.14-1.42 seconds, and warm
`all` requests completed in 1.12-1.56 seconds. Warm results were not
consistently faster than the copy-backed-cold proxy, so this evidence does not
support cold I/O as the primary cause of the reported timeout.

The current production shape still spends most of its direct-query time on the
full `usage_event` projection, but row volume being the dominant component is
not equivalent to proving an uncached-file I/O regression. Per `implement.md`,
the task must stop at this gate and revise the diagnosis/PRD before any D1/D2
product change.

## Method

- Captured at `2026-07-29T03:27:47Z` on Windows, from Git HEAD
  `6902f4460d9ef24c934cc7e6aa3a338396249869`.
- Binary: `llmusage 1.1.1`, SHA-256
  `4a0595b068e0f4a311959dbba8e37abc997cb4dae195e969bbff86bcefa8b347`.
- The source database was opened with SQLite `mode=ro` and `query_only=ON`, then
  copied with the SQLite online-backup API. All profiling and HTTP servers used
  the backup or descendants under `target/tmp/activity-baseline-step1-run2/`.
  The live database was not bootstrapped, migrated, or written.
- Snapshot: schema v18, 1,160,073,216 bytes, `PRAGMA quick_check=ok`, 178,795
  `usage_event` rows and 174,569 `usage_turn` rows.
- Copy-backed-cold: a new database file copy and a new debug server process for
  every measured round; no API request preceded the measurement.
- Warm: one copied database and one server process per cell, followed by one
  unmeasured request round and five measured rounds.
- Solo issued one request per round. Concurrency-2 released two request threads
  together and retained both request timings. Each of the eight cells has five
  rounds; there are 40 rounds and 60 requests in total.
- Only HTTP status, support level, timeout/degraded booleans, wall time, and the
  server's `semaphore_wait_ms`, `query_ms`, and `cancelled` fields were retained.
  No response rows, model names, paths, project hashes, or session identifiers
  were persisted.

### Cold-proxy limitation

This run did **not** evict the Windows OS file cache. Safe per-file eviction is
not available through the standard Windows/Python interfaces; the practical
alternatives are a reboot or privileged host-wide standby/file-cache eviction.
Neither was appropriate for this scoped diagnostic. A fresh copy gives each
round a new file and process but the copy operation itself may populate cache
pages, so these measurements must be called `copy-backed-cold`, not true
uncached cold-start measurements.

## Summary

Medians below are across individual requests. Concurrency-2 cells therefore
contain ten request observations across five rounds.

| Temperature | Range | Load | Wall ms min / median / max | Query ms min / median / max | Wait max | Degraded / requests |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| copy-backed-cold | `1d` | solo | 854.87 / 909.99 / 926.89 | 851 / 873 / 906 | 0 | 0 / 5 |
| warm | `1d` | solo | 799.70 / 848.45 / 921.03 | 792 / 831 / 896 | 0 | 0 / 5 |
| copy-backed-cold | `1d` | concurrency-2 | 811.54 / 892.21 / 1387.84 | 806 / 858 / 1383 | 0 | 0 / 10 |
| warm | `1d` | concurrency-2 | 1021.79 / 1295.14 / 2304.45 | 1019 / 1292 / 2301 | 0 | 0 / 10 |
| copy-backed-cold | `all` | solo | 1140.84 / 1267.71 / 1411.51 | 1123 / 1239 / 1408 | 0 | 0 / 5 |
| warm | `all` | solo | 1119.75 / 1400.80 / 1551.34 | 1117 / 1389 / 1549 | 0 | 0 / 5 |
| copy-backed-cold | `all` | concurrency-2 | 1250.30 / 1323.28 / 1385.03 | 1245 / 1319 / 1381 | 0 | 0 / 10 |
| warm | `all` | concurrency-2 | 1364.92 / 1474.91 / 1557.66 | 1362 / 1460.5 / 1555 | 0 | 0 / 10 |

All 60 responses were HTTP 200 with normalized support. No response carried a
timeout reason; no server metric reported cancellation. Permit contention is
not the explanation in this run because every `semaphore_wait_ms` was zero.

The solo `1d` medians satisfy the existing sub-second target. The warm
concurrency-2 `1d` median is 1.295 seconds and therefore does not satisfy that
budget if the budget is applied to concurrent observations. All `all` samples
remain below the three-second deadline.

## Raw HTTP samples

Pairs are the two requests released together in one concurrency-2 round.
`wait` was `0` for every value; `degraded`, `timeout`, and `cancelled` were
`false` for every value. The client results and server log metrics have no
shared request ID, so the two wall values and two query values are complete
per-round multisets; their positions within a pair are not a correlation.

| Temperature | Range | Load | Round | Wall ms | Query ms |
| --- | --- | --- | ---: | ---: | ---: |
| copy-backed-cold | `1d` | solo | 1 | 926.89 | 861 |
| copy-backed-cold | `1d` | solo | 2 | 909.99 | 906 |
| copy-backed-cold | `1d` | solo | 3 | 919.29 | 890 |
| copy-backed-cold | `1d` | solo | 4 | 876.89 | 873 |
| copy-backed-cold | `1d` | solo | 5 | 854.87 | 851 |
| warm | `1d` | solo | 1 | 799.70 | 797 |
| warm | `1d` | solo | 2 | 809.89 | 792 |
| warm | `1d` | solo | 3 | 855.09 | 839 |
| warm | `1d` | solo | 4 | 848.45 | 831 |
| warm | `1d` | solo | 5 | 921.03 | 896 |
| copy-backed-cold | `1d` | concurrency-2 | 1 | 957.79 / 866.65 | 863 / 845 |
| copy-backed-cold | `1d` | concurrency-2 | 2 | 815.17 / 811.54 | 806 / 811 |
| copy-backed-cold | `1d` | concurrency-2 | 3 | 856.92 / 932.15 | 853 / 825 |
| copy-backed-cold | `1d` | concurrency-2 | 4 | 893.22 / 891.19 | 887 / 890 |
| copy-backed-cold | `1d` | concurrency-2 | 5 | 1334.67 / 1387.84 | 1330 / 1383 |
| warm | `1d` | concurrency-2 | 1 | 1037.36 / 1021.79 | 1019 / 1034 |
| warm | `1d` | concurrency-2 | 2 | 1298.42 / 1291.87 | 1288 / 1296 |
| warm | `1d` | concurrency-2 | 3 | 1301.63 / 1282.30 | 1279 / 1299 |
| warm | `1d` | concurrency-2 | 4 | 2304.45 / 2246.73 | 2244 / 2301 |
| warm | `1d` | concurrency-2 | 5 | 1314.20 / 1271.70 | 1269 / 1311 |
| copy-backed-cold | `all` | solo | 1 | 1411.51 | 1408 |
| copy-backed-cold | `all` | solo | 2 | 1286.06 | 1282 |
| copy-backed-cold | `all` | solo | 3 | 1174.12 | 1170 |
| copy-backed-cold | `all` | solo | 4 | 1140.84 | 1123 |
| copy-backed-cold | `all` | solo | 5 | 1267.71 | 1239 |
| warm | `all` | solo | 1 | 1119.75 | 1117 |
| warm | `all` | solo | 2 | 1455.12 | 1437 |
| warm | `all` | solo | 3 | 1397.56 | 1384 |
| warm | `all` | solo | 4 | 1400.80 | 1389 |
| warm | `all` | solo | 5 | 1551.34 | 1549 |
| copy-backed-cold | `all` | concurrency-2 | 1 | 1385.03 / 1357.27 | 1353 / 1381 |
| copy-backed-cold | `all` | concurrency-2 | 2 | 1356.37 / 1363.06 | 1352 / 1359 |
| copy-backed-cold | `all` | concurrency-2 | 3 | 1250.30 / 1253.75 | 1245 / 1250 |
| copy-backed-cold | `all` | concurrency-2 | 4 | 1334.21 / 1312.36 | 1308 / 1330 |
| copy-backed-cold | `all` | concurrency-2 | 5 | 1299.49 / 1296.15 | 1292 / 1296 |
| warm | `all` | concurrency-2 | 1 | 1367.28 / 1364.92 | 1362 / 1365 |
| warm | `all` | concurrency-2 | 2 | 1523.51 / 1521.49 | 1519 / 1521 |
| warm | `all` | concurrency-2 | 3 | 1456.15 / 1452.55 | 1450 / 1454 |
| warm | `all` | concurrency-2 | 4 | 1546.92 / 1557.66 | 1544 / 1555 |
| warm | `all` | concurrency-2 | 5 | 1484.13 / 1465.69 | 1463 / 1458 |

## Direct projection profile

These are five warm-ish read-only iterations against the online-backup
snapshot after its metadata/quick-check pass. They describe the production
statement shape and must not be presented as uncached timings.

| Range | Production statement | Rows | Plan | Median ms | Samples ms |
| --- | --- | ---: | --- | ---: | --- |
| `1d` | event cost projection | 178,795 | `SCAN usage_event` | 598.32 | 590.61, 598.32, 549.68, 648.09, 615.77 |
| `1d` | filtered turn projection | 7,072 | `SEARCH ... idx_usage_turn_started_at` | 18.52 | 18.52, 17.68, 20.06, 16.94, 18.63 |
| `all` | event cost projection | 178,795 | `SCAN usage_event` | 573.78 | 573.78, 655.20, 601.56, 549.25, 534.77 |
| `all` | turn projection | 174,569 | `SCAN t` | 382.50 | 394.17, 450.67, 382.50, 362.23, 357.08 |

The full event projection is paid even for `1d`, while the bounded turn range
uses `idx_usage_turn_started_at`. This supports the narrower statement that the
event projection dominates Activity query work. It does not establish that
uncached disk I/O is what pushed a prior request past three seconds.

## Reproduction artifacts

- Harness: `research/profile_activity_baseline.py`
- Sanitized raw result: `target/tmp/activity-baseline-step1-run2/results.json`
- Server stderr/stdout: per-runtime `profile-logs/` directories under the same
  ignored target path

No D1 index experiment, D2 query rewrite, migration, spec update, or product
code change was performed.
