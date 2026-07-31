# Activity D1 isolated index experiment

Captured: `2026-07-30T03:22:08Z`

Candidate: `CREATE INDEX idx_usage_event_activity_cost ON usage_event(event_key, cost_with_cache_usd)`

Index build: `3993.71 ms`; physical database delta: `0 bytes`; physical page delta: `0`; active page delta: `4256` (`17432576 bytes`); freelist delta: `-4256`; schema version unchanged: `True`.

Plan before: `SCAN usage_event`

Plan after: `SCAN usage_event USING COVERING INDEX idx_usage_event_activity_cost`

| Attempt | Wall ms | HTTP | Support | Degraded | Timeout | Wait ms | Query ms | Cancelled |
| ---: | ---: | ---: | --- | --- | --- | ---: | ---: | --- |
| 1 | 2127.34 | 200 | normalized | False | False | 0 | 2094 | False |
| 2 | 657.64 | 200 | normalized | False | False | 0 | 632 | False |
| 3 | 737.83 | 200 | normalized | False | False | 0 | 716 | False |
| 4 | 681.14 | 200 | normalized | False | False | 0 | 665 | False |
| 5 | 702.87 | 200 | normalized | False | False | 0 | 701 | False |

## Write amplification probe

Baseline rollback probe WAL after insert+update: `770472 bytes`; indexed rollback probe WAL: `828152 bytes`; ratio: `1.075`.

Both probes used 100 transient inserts plus 100 transient updates and rolled back. This is local page/WAL evidence, not an end-to-end sync throughput benchmark.

The index consumed 4,256 pages from the existing freelist, so the physical file and total `page_count` stayed constant while active allocation increased by 17,432,576 bytes. The combined rollback-probe WAL changed from 770,472 to 828,152 bytes, a 1.075 ratio (+7.5%). The insert-only intermediate WAL changed from 341,992 to 379,072 bytes.

All five HTTP requests were normalized and non-degraded, with zero permit wait and no SQLite busy/locked signal. The first fresh-server request was 2,127.34 ms; the next four were 657.64-737.83 ms.

## Assessment and limits

This isolated result supports carrying D1 forward for a production design decision: it changes the event projection from a table scan to the intended covering-index scan, stays within the 3-second `all` HTTP boundary in this experiment, uses 16.62 MiB of active pages, and adds 7.5% WAL bytes in the representative rollback probe.

It is not a reboot-cleared cold-cache measurement. Building the index reads the table and writes index pages before the fresh server starts, so Windows may retain both in cache. The probe is also not an end-to-end sync throughput benchmark. Finally, no response rows were retained and no byte-for-byte output comparison was run in this D1-only experiment. Those checks remain mandatory before any migration can be accepted.

The current product source, migration set, query/reducer, cache, and timeout were unchanged. The D1 database copy and sidecars were removed after retaining sanitized evidence. A user decision is required before any production implementation.

## Command

```powershell
python -B '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/profile_activity_confirmation.py' d1 --confirmation-results '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/confirmation-results.json' --snapshot 'target/tmp/activity-baseline-step1-run2/snapshot/llmusage.db' --binary 'target/debug/llmusage.exe' --work-dir 'target/tmp/activity-timeout-d1-v2' --output '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/d1-results.json' --validation '.trellis/tasks/07-29-fix-behavior-activity-timeout/research/d1-validation.md'
```
