# Ready Widgets API Baseline

Captured 2026-08-03 from `http://127.0.0.1:37503` with
`timezone=Asia/Shanghai` against the local dashboard database.

## Node baseline

- `mise exec node@22 -- node --test scripts/tests/*.test.mjs`
- Result: 33 tests passed, 0 failed.
- The plan's directory-form `node --test scripts/tests/` is not supported by
  Node 22 on Windows, so the repository test glob was used.
- Lifecycle assertions derive `secondaryTotal` from `SECONDARY_SECTIONS`; one
  test title hard-codes the current count as "five secondary loaders".

## Home overview

The first cold request reached the existing 5 second web timeout. A warm retry
returned HTTP 200 with 39,298 bytes in 4.253766 seconds.

```json
{
  "summary": {
    "total_sessions": 2757,
    "total_requests": 198828,
    "total_tokens": 26760676199,
    "total_cost_usd": 26056.326798040172,
    "cache_efficiency": 0.9338142037883636,
    "active_days": 155,
    "platforms": 7
  },
  "by_platform": {
    "antigravity": { "sessions": 94, "requests": 1304, "tokens": 144402257 },
    "claude": { "sessions": 363, "requests": 33935, "tokens": 6043522829 },
    "codex": { "sessions": 2022, "requests": 156838, "tokens": 19901648810 },
    "grok": { "sessions": 28, "requests": 46, "tokens": 2869275 },
    "kimi_code": { "sessions": 9, "requests": 3265, "tokens": 337986732 },
    "opencode": { "sessions": 238, "requests": 3390, "tokens": 327618677 },
    "pi": { "sessions": 3, "requests": 50, "tokens": 2627619 }
  }
}
```

## Heatmap

`GET /api/heatmap?days=366&timezone=Asia%2FShanghai` returned 366 rows. The
first three non-zero rows were:

```json
[
  { "date": "2025-10-14", "event_count": 30, "total_tokens": 557928 },
  { "date": "2025-11-06", "event_count": 1, "total_tokens": 5811 },
  { "date": "2025-11-15", "event_count": 7, "total_tokens": 67127 }
]
```

## Daily trends

`GET /api/trends_daily?timezone=Asia%2FShanghai` returned 155 rows. The first
three rows were:

```json
[
  { "date": "2025-10-14", "input_tokens": 302874, "cache_read_tokens": 215112, "cache_creation_tokens": 0, "output_tokens": 7796, "total_tokens": 557928, "event_count": 30, "cost_with_cache_usd": 0.0 },
  { "date": "2025-11-06", "input_tokens": 5331, "cache_read_tokens": 0, "cache_creation_tokens": 0, "output_tokens": 29, "total_tokens": 5811, "event_count": 1, "cost_with_cache_usd": 0.0 },
  { "date": "2025-11-15", "input_tokens": 21760, "cache_read_tokens": 44172, "cache_creation_tokens": 0, "output_tokens": 753, "total_tokens": 67127, "event_count": 7, "cost_with_cache_usd": 0.0 }
]
```

## Release performance after schema v20 covering index

Measured 2026-08-03 against the same 1.16 GB database (`198,828`
`usage_event` rows, `5,075` buckets). Each row below follows one unrecorded
warm-up request and contains three recorded samples. The final rebuilt
`target/release/llmusage.exe` migrated a workspace-only online backup from v19
to v20 and served it on port `37508`; the port was stopped after sampling.

Schema v20 creates exactly one event-at-first covering expression index:

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

The index built in `1.281s` and occupied 8,123 logical pages:
`33,271,808` bytes (`31.730 MiB`). SQLite reused freelist pages, so the main
database file did not grow. Post-migration schema was v20, the only matching
home index was `idx_usage_event_home_compact_cover`, and `PRAGMA quick_check`
returned `ok`. EXPLAIN reported a covering index scan for `all` and a covering
`event_at` range search for date-bounded requests.

| Endpoint | Range | Times | Bytes | 400 ms result |
| --- | --- | --- | ---: | --- |
| `/api/home_overview?compact=true` | `1d` | 4.110 / 4.294 / 4.298 ms | 356 | pass |
| `/api/home_overview?compact=true` | `7d` | 30.952 / 30.270 / 31.707 ms | 600 | pass |
| `/api/home_overview?compact=true` | `30d` | 110.530 / 111.281 / 108.657 ms | 612 | pass |
| `/api/home_overview?compact=true` | `all` | 188.632 / 201.187 / 222.342 ms | 630 | pass |
| `/api/heatmap?days=366` | fixed | 13.17 / 14.07 / 14.01 ms | 21,455 | pass |
| `/api/trends_daily` | `all` | 13.89 / 15.11 / 15.68 ms | 30,624 | pass |

Compact/full comparison retained exact integer fields, `by_platform` keys and
values, and response structure for all four ranges. The 1d/7d/30d float fields
were byte-equivalent. For `all`, only `summary.total_cost_usd` differed due to
index scan accumulation order, by `1.8189894035458565e-11`; this is within the
existing `EPSILON = 1e-9` contract. The rejected all-range table-order cost
rescan restored byte equality but regressed `all` to 673-770ms, so it was not
adopted. The rejected identity-first second index would add another 31.703 MiB
without solving that bottleneck, so v20 intentionally contains no second
index.

The retained compact implementation streams one exact filtered
`usage_event` scan into a single accumulator and does not introduce
`DISTINCT`, `GROUP BY`, or temporary B-trees. It skips the full response's
series, run-state, and archive diagnostics work. A compact-only SQL aggregate
experiment using distinct temporary sets measured 684-1,065 ms for `all`, so
it was rejected and the exact streaming form was restored.

A final split-query experiment was also rejected. Its numeric phase planned
`SCAN usage_event` plus `USE TEMP B-TREE FOR GROUP BY`; session identity used
`idx_usage_event_source_path_hash` plus a distinct temporary B-tree; active
days used the `idx_usage_event_event_at` covering index plus another distinct
temporary B-tree. Despite matching the full projection in the focused filter
test, release samples were 460-508 ms (`1d`), 589-767 ms (`7d`), 1,046-1,162
ms (`30d`), and 1,694-1,855 ms (`all`). The experiment and its temporary
profiling instrumentation were removed.

Schema v20 resolves the earlier `30d`/`all` performance failure with the single
covering expression index documented above; no maintained aggregate or second
home-overview index is required.
