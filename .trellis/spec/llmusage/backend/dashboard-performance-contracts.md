# Dashboard Performance Contracts

## Scenario: Interactive time-range refresh

### 1. Scope / Trigger

- Apply this contract when changing `/api/dashboard`, dashboard range controls,
  secondary dashboard endpoints, Explorer query routing, or SQLite query
  cancellation.
- The interactive path is additive. Full dashboard and static snapshot/export
  payloads remain complete and backward compatible.
- The representative-data budgets are: click feedback p95 at most 100 ms,
  interactive API p95 at most 400 ms after one warm-up, and interactive JSON at
  most 128 KiB.

### 2. Signatures

```text
GET /api/dashboard?scope=interactive&range=<1d|7d|30d|all>&window=<day|week|month|all>
Dashboard::interactive_snapshot(&QueryFilter, window: &str)
    -> Result<DashboardInteractiveSnapshot>
load_via_dashboard(state, section, query) -> Future<Result<T>>
```

`QueryFilter` fields shared by bucket and fact queries are `source`, `model`,
`project_hash`, `since`, `until`, and `timezone`.

### 3. Contracts

- The interactive response contains exactly one selected `trends` series plus
  `overview`, `models`, `sources`, `projects`, `costs`,
  `sync_command_center`, `diagnostics`, and `health`.
- Interactive `health` is a summary with `cursor_count`; it must not serialize
  the full cursor array. Full and core contracts keep their existing shapes.
- A range click updates selected/loading state before its first `await`, aborts
  the previous generation, and accepts results only when both generation and
  stable filters still match.
- Secondary `activity`, `tools`, `optimize`, `explorer`, and `compare` requests
  run independently with concurrency `2`. They may update only their own
  section and must retain stale/loading metadata until all current-generation
  sections settle.
- Live response caching keeps normalized request keys for 10 seconds, is
  capped at 32 entries, and aborts in-flight requests during invalidation.
- Server-side dashboard work remains on `spawn_blocking`, holds one of four
  query permits for the blocking task lifetime, publishes an SQLite
  `InterruptHandle`, and interrupts plus awaits abandoned work before releasing
  the permit.
- Explorer uses `usage_bucket_30m` only for source/model/project groupings with
  attributed cost, calls, or total-token metrics and no session/tool/is-tool/
  token-type filters. Other shapes keep event, turn, or attribution strategies.
- Source totals come from `usage_bucket_30m`. `SourceBreakdown.last_event_at`
  remains the exact filtered `MAX(usage_event.event_at)` and must be queried per
  returned source so `(source, event_at)` can be used.
- Debug timing fields are `section`, `semaphore_wait_ms`, `query_ms`, and
  `cancelled`; API serialization adds `endpoint`, `serialization_ms`, and
  `payload_bytes`.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Unknown or omitted `scope` | Use the existing full dashboard contract |
| `scope=core` | Use the existing core contract |
| `scope=interactive` | Return the lean selected-window contract |
| Unknown `range`/`window` | Normalize to `day` |
| Browser abort or newer generation | Do not publish the obsolete result |
| Query timeout before/after handle publication | Interrupt when possible, await the blocking task, return the structured timeout error |
| SQLite reports `OperationInterrupted` | Map to `LlmusageError::Cancelled`, not configuration failure |
| Semaphore closes | Return structured `ConfigInvalid` detail |
| Secondary section fails | Keep other sections usable and mark only that section degraded/stale |
| Explorer query is not bucket compatible | Route to the corresponding fact strategy without approximation |

### 5. Good/Base/Bad Cases

- Good: `scope=interactive&range=7d&window=week&source=codex` returns one
  weekly trend, no cursor rows, and filtered source/model/project totals.
- Base: a full `/api/dashboard` request still returns all historical trend
  windows and secondary sections for compatibility.
- Bad: a rapid `1d -> 7d -> 30d -> all` sequence lets a slower `1d` response
  overwrite the selected `all` state or leaves its SQLite statement running.
- Bad: an Explorer request with `session_id` reads buckets and silently drops
  session semantics.

### 6. Tests Required

- Rust contract tests assert interactive fields, one selected trend, no cursor
  array, and unchanged full/core behavior.
- Rust cancellation tests force a slow SQLite statement, assert interruption,
  and prove the semaphore permit is released.
- Explorer equivalence tests compare bucket and event totals, rows, Other,
  series, filters, and timezone boundaries for supported shapes; routing tests
  assert fact-only shapes do not use buckets.
- Source breakdown tests assert source/model/project/date filters and exact
  `last_event_at` values.
- Node request-lifecycle tests assert normalized coalescing, AbortSignal
  propagation, in-flight invalidation, and the 32-entry cache bound.
- Run `node scripts/benchmark-dashboard-range.mjs --url <dashboard-url>
  --iterations 5 --output <task-evidence.json>` against representative data and
  assert every range meets the API and payload budgets.
- Run `just ci` before completion.

### 7. Wrong vs Correct

#### Wrong

```sql
SELECT source, MAX(event_at)
FROM usage_event
GROUP BY source;
```

This scans and groups the full event table even when only a few sources exist.

#### Correct

```sql
SELECT source, SUM(event_count)
FROM usage_bucket_30m
GROUP BY source;

SELECT MAX(event_at)
FROM usage_event
WHERE source = ?;
```

The first query uses the aggregate projection. The second runs once per
returned source, preserves exact fact semantics, and can use
`idx_usage_event_source_event_at`.
