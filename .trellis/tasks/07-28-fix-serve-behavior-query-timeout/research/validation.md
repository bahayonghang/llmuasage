# Representative Behavior validation

## Safety boundary

- Before the first live bootstrap, the live database was opened read-only for
  schema/count verification and confirmed at schema v17.
- The server and every HTTP measurement used the isolated runtime copy at
  `D:\Documents\Code\CLI\llmusage\target\tmp\behavior-query-v18-copy-agent`.
- That runtime copy was schema v17 with `PRAGMA integrity_check=ok` before the
  copied server bootstrap, but it was then migrated to v18 for validation and
  is not the retained rollback backup.
- The copied server migrated only that database to schema v18. A post-migration
  integrity check returned `ok`, and all seven v18 indexes were present:
  `idx_usage_event_event_at`, `idx_usage_turn_started_at`,
  `idx_usage_turn_session_id`, `idx_usage_turn_event_key_expr`,
  `idx_usage_tool_call_event_key`, `idx_usage_tool_call_occurred_at`, and
  `idx_usage_tool_call_model_occurred`.
- The retained online backup created before any live migration is
  `C:\Users\lyh\.llmusage\backups\llmusage.db.pre-schema-v18-20260729-010307.sqlite`.
  It remains schema v17, opens read-only with `PRAGMA integrity_check=ok`, is
  1,160,073,216 bytes, and contains 176,898 events, 172,676 turns, and 157,903
  tool calls. Its SHA-256 is
  `93b0b4bfc60f53e1fd798ae9e8e117a4d298c43c5be5327ac608f325c99ef915`.
- Only after that retained backup passed read-only verification, the task-owned
  server bootstrapped the live database to schema v18. The migrated live
  database passed `PRAGMA integrity_check=ok` and exposed all seven expected
  indexes. The validation server was then stopped and port 37431 was clear.

Operational rollback after a v18 bootstrap is to restore the database from the
retained pre-v18 backup above or continue with a v18-aware binary. Dropping
indexes alone does not restore schema compatibility with older binaries.

## Baseline

The clean-server reproduction and read-only direct-query timings are recorded in
`diagnosis.md`. Before the fix all four unbounded HTTP sections degraded at the
one-second deadline. Representative direct SQL took about 2,996 ms for Activity
and 10,764 ms for Tools; Optimize's dominant low-read/edit and session-outlier
queries took about 1,626 ms and 2,284 ms respectively.

An intermediate post-rewrite HTTP run rejected the first bounded Tools shape:
although it no longer timed out, its `1d` median was 1,070.27 ms. The final path
loads range-matching events first and fetches only additional filtered-tool event
keys, while retaining the sequential full-event scan for `all`.

## Final HTTP measurements

The debug binary served the copied database on loopback. Each section was called
three times with `range=1d` and three times with `range=all`; timings are full
client-observed HTTP wall time in milliseconds. Only status/support fields and
timings were inspected. No model names, paths, sessions, or response bodies were
persisted.

| Section | `1d` samples | `1d` median | `all` samples | `all` median |
| --- | ---: | ---: | ---: | ---: |
| Activity | 868.54, 814.10, 811.79 | 814.10 | 1271.52, 1301.19, 1267.83 | 1271.52 |
| Tools | 102.57, 94.80, 90.98 | 94.80 | 2462.15, 2526.53, 2432.88 | 2462.15 |
| Optimize | 173.02, 183.65, 163.08 | 173.02 | 589.71, 571.90, 559.49 | 571.90 |
| Compare | 31.31, 37.22, 31.44 | 31.44 | 538.01, 526.58, 508.48 | 526.58 |

Every request returned HTTP 200. Activity, Tools, and Optimize reported
`normalized`; Compare reported its valid `low_sample` state. No support reason
contained `timeout`, every `1d` median was below one second, and every `all`
sample completed below the three-second Behavior deadline.

## Concurrency two

The four `range=all` sections were loaded through a queue with a concurrency
limit of two, matching the Behavior browser fan-out limit.

| Section | Wall time (ms) | HTTP | Support |
| --- | ---: | ---: | --- |
| Activity | 1337.77 | 200 | normalized |
| Tools | 2483.22 | 200 | normalized |
| Optimize | 595.31 | 200 | normalized |
| Compare | 450.88 | 200 | low_sample |

The concurrency-two round completed in 2517.74 ms. No request degraded or
crossed the three-second section deadline.

## Live database validation

After the retained v17 backup passed its safety gate, the debug binary served
the migrated live database on loopback port 37431. The same aggregate-only
checks produced the following client-observed results:

| Section | `1d` samples | `1d` median | `all` samples | `all` median |
| --- | ---: | ---: | ---: | ---: |
| Activity | 651.49, 1186.33, 665.39 | 665.39 | 1098.35, 1104.68, 1078.73 | 1098.35 |
| Tools | 72.48, 76.78, 80.18 | 76.78 | 2362.32, 2257.23, 2505.15 | 2362.32 |
| Optimize | 137.74, 132.98, 123.82 | 132.98 | 593.98, 564.06, 533.75 | 564.06 |
| Compare | 24.55, 29.24, 27.25 | 27.25 | 523.09, 496.98, 511.51 | 511.51 |

Every request returned HTTP 200. Activity, Tools, and Optimize were
`normalized`; Compare was valid `low_sample`. No reason contained `timeout`,
every `1d` median was below one second, and every `all` sample was below three
seconds.

The live concurrency-two `all` round completed in 2497.33 ms. Activity took
1284.25 ms, Tools 2482.50 ms, Optimize 622.70 ms, and Compare 483.23 ms; every
response remained HTTP 200 with the same supported/low-sample levels and no
timeout reason.

## Browser DOM verification

An isolated Chromium session opened the live dashboard and selected the
Behavior anchor. The settled DOM contained four Activity rows and eight Tools
rows with `normalized` support, one normalized Optimize finding, and eight
Compare metric rows with the expected `low_sample` warning. The Behavior region
contained no loading text and the rendered page contained no timeout reason.
A full-page screenshot was inspected for overlap before the browser session and
task-owned server were closed.
