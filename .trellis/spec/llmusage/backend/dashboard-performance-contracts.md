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
TUI PanelRequest(panel, filter, time_window, generation, refreshing)
    -> bounded PanelResult channel
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
- TUI panel reads follow the same cancellation boundary from its synchronous
  event loop: every request opens a fresh `Dashboard` inside `spawn_blocking`,
  holds one of five TUI-local permits, publishes an interrupt handle through a
  shared slot, and sends one typed result through a bounded channel. The UI
  thread never opens a dashboard connection or waits for a query.
- TUI result acceptance requires panel, generation, time window, and every
  stable `QueryFilter` field to match current state. A cold request keeps the
  payload empty so the loading frame is reachable; a forced refresh retains
  the current payload and marks it stale until the matching result arrives.
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
| TUI switch/filter/window supersedes a request | Interrupt it and discard any late result that fails the full request match |
| TUI refresh with cached payload | Keep rendering cached data and expose refreshing status |
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
- Good: switching from Stats to Blocks immediately paints the Blocks loading
  state; a late Stats result is discarded after its SQLite statement is
  interrupted.
- Bad: a TUI key handler calls a `Dashboard` query before returning to draw,
  making the existing loading branch unreachable.
- Bad: an Explorer request with `session_id` reads buckets and silently drops
  session semantics.

### 6. Tests Required

- Rust contract tests assert interactive fields, one selected trend, no cursor
  array, and unchanged full/core behavior.
- Rust cancellation tests force a slow SQLite statement, assert interruption,
  and prove the semaphore permit is released.
- TUI tests force a slow SQLite statement, assert bounded cancellation, reject
  stale generation/filter results, render cold loading states through
  `TestBackend`, and compare parallel Stats/Behavior payloads to serial reads.
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

## Scenario: Cold home overview query

`Dashboard::home_overview` is a cold read contract, not a cacheable or
pre-warmed projection. The local seeded 10k-event test retains its strict
80 ms budget in both debug and release builds; CI-only tolerance must not be
used as completion evidence.

The query must preserve the exact event semantics for `QueryFilter` source,
model, project, date bounds, and fixed/local timezone conversion. Session
identity is `source` plus the first non-empty value of `session_id`,
`source_path_hash`, or `event_key`. A session may appear on multiple local
calendar days: it counts once in the summary and once per day/source in the
series. The stable by-platform map always contains `claude`, `codex`,
`antigravity`, and `opencode`, while unknown sources remain compatible in the
map and are omitted from the fixed series fields.

The summary, by-platform, and daily series sections share one filtered
`usage_event` row stream. Rust-side aggregation may build the three exact
projections, but a change must not reintroduce three independent fact-table
scans or distinct/group temporary B-trees. Test-only profiling records total,
event-read, summary, by-platform, series, run-state, and diagnostics elapsed
time plus `EXPLAIN QUERY PLAN` details and opcode count. Production payloads do
not expose these fields.

Archive diagnostics may aggregate `usage_bucket_30m` only for sources with
missing source files; when all files are live, the bucket scan is skipped.
This is a query-path optimization with no schema or migration change, and it
must retain protected-event counts and archive payload fields exactly.

Validation requires the focused 80 ms test to pass three consecutive times in
debug and release, exact cross-day/session/filter coverage, and a read-only or
online-backup profile for representative databases. No process cache, warm-up
query, delayed work, platform exception, or threshold relaxation is allowed.

The first query uses the aggregate projection. The second runs once per
returned source, preserves exact fact semantics, and can use
`idx_usage_event_source_event_at`.
