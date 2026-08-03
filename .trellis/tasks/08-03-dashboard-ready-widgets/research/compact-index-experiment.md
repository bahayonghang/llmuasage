# Compact home overview covering-index experiment

Date: 2026-08-03. All experiments used a SQLite online backup under
`target/tmp/home-overview-index-exp-v1`; the real
`C:\Users\lyh\.llmusage\llmusage.db` was opened only as the online-backup
source and was not migrated or written.

## Fixture integrity

- source size: 1,160,073,216 bytes, unchanged before/after backup
- source mtime: unchanged before/after backup
- backup time: 6.422s
- backup schema: v19
- backup rows: 198,828 `usage_event`
- backup `PRAGMA quick_check`: `ok`

## Adopted candidate

```sql
CREATE INDEX idx_usage_event_home_compact_cover
ON usage_event(
    event_at,
    source,
    model,
    project_hash,
    COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key),
    input_tokens,
    cache_creation_tokens,
    cache_read_tokens,
    total_tokens,
    cost_with_cache_usd
);
```

- build time: 1.281s
- logical footprint: 8,123 pages / 33,271,808 bytes / 31.730 MiB
- main DB file size did not grow because SQLite reused freelist pages
- post-build `PRAGMA quick_check`: `ok`
- all range plan: covering index scan
- date range plan: covering index search on `event_at`
- combined source/date/model/project plan: planner preferred existing
  `idx_usage_event_source_event_at`

Release timings after one warm-up, three measured samples:

| Range | Samples (ms) |
| --- | --- |
| 1d | 5.509 / 5.315 / 6.231 |
| 7d | 36.603 / 41.511 / 36.380 |
| 30d | 159.138 / 152.879 / 153.862 |
| all | 240.001 / 226.386 / 222.948 |

The 1d/7d/30d compact projections were byte-equivalent to the full projection.
For all range, only `summary.total_cost_usd` differed; absolute delta was
`1.8189894035458565e-11`, within the existing `EPSILON = 1e-9`. Integer
fields, map keys, and structure remained exact.

## Rejected alternatives

- Source-first covering index: 31.69 MiB, 2.086s; all range passed at
  215–232ms, but 30d failed at 439–480ms.
- Identity-first second index: extra 31.703 MiB and 1.444s build; removed a
  session DISTINCT temporary B-tree but did not fix the exact-cost rescan.
- All-range table-order cost rescan: restored byte equality but regressed all
  range to 673–770ms.
- Split SQL aggregates: introduced temporary B-trees and was slower than the
  single streamed event scan.

Decision: migrate only the event-at-first covering index in schema v20. Normal
rollback drops `idx_usage_event_home_compact_cover`; binary downgrade restores
the pre-v20 backup rather than editing `schema_version` manually.

## Final-source validation

After implementation, both experimental indexes were dropped from the
workspace backup, returning it to schema v19 with `quick_check=ok`. The rebuilt
release binary's supported `init` path then migrated that backup to v20.

- schema after bootstrap: v20
- matching indexes: only `idx_usage_event_home_compact_cover`
- post-migration `PRAGMA quick_check`: `ok`
- all-range plan: `SCAN usage_event USING COVERING INDEX idx_usage_event_home_compact_cover`
- date-range plan: `SEARCH usage_event USING COVERING INDEX idx_usage_event_home_compact_cover (event_at>? AND event_at<?)`

Loopback HTTP measurements from the rebuilt release binary, after one warm-up:

| Range | Samples (ms) | HTTP bytes |
| --- | --- | --- |
| 1d | 4.110 / 4.294 / 4.298 | 356 |
| 7d | 30.952 / 30.270 / 31.707 | 600 |
| 30d | 110.530 / 111.281 / 108.657 | 612 |
| all | 188.632 / 201.187 / 222.342 | 630 |

All four ranges retained exact integer fields, `by_platform` keys/values, and
response structure. Cost and cache-efficiency deltas were zero for 1d/7d/30d;
all-range cost delta remained `1.8189894035458565e-11` and cache-efficiency
delta remained zero. The temporary server was stopped after measurement.
