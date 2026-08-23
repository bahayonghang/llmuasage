# `/api/sessions` performance evidence

Date: 2026-08-23

## Safety boundary

- The active user database was never opened by the benchmark server.
- Python's SQLite online-backup API copied it into a temporary runtime root in read-only source mode.
- Source size: `1,160,073,216` bytes.
- Backup verification: `PRAGMA integrity_check = ok`.
- The local server used the copied runtime root and listened only on `127.0.0.1`.

## Representative default-range benchmark

Endpoint shape:

```text
GET /api/sessions?limit=10&range=1d&sort=<tokens|duration|cost>
```

Method: one warm-up followed by five sequential HTTP samples per sort. The
reported p95 is the nearest-rank p95 (the maximum of five samples). Wall-clock
time includes the loopback HTTP boundary and JSON serialization. Payload size
is the decoded UTF-8 JSON size.

| Sort | Samples (ms) | p95 (ms) | Maximum payload | Result |
| --- | --- | ---: | ---: | --- |
| tokens | 96.63, 96.52, 101.15, 93.85, 100.29 | 101.15 | 3,954 bytes | PASS |
| duration | 30.77, 31.66, 34.31, 35.63, 41.60 | 41.60 | 3,889 bytes | PASS |
| cost | 90.00, 89.49, 96.04, 93.36, 95.14 | 96.04 | 3,962 bytes | PASS |

All three supported responses satisfy the task budgets of `<= 400 ms` and
`<= 128 KiB` for the dashboard's default `1d` range.

## Bounded limitation discovered

An exploratory unbounded request against the same large backup did not meet
the interactive budget: token and cost sorts reached the existing 3-second
section timeout, while one duration sample completed in about 2.5 seconds.
This task did not add a query, scan, index, or schema change: it only exposes
two timestamps already selected by the existing Top Sessions query. Improving
unbounded Top Sessions on a 1.16 GB database requires a separately designed
query/index change and remains outside this UI-correction task. The default
range evidence above must not be read as an all-range performance claim.
