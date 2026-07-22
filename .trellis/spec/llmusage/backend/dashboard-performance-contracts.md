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
TimeWindow::query_filter(&QueryFilter)
    -> QueryFilter with inclusive local-calendar since/until dates
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
- TUI windows are `Today`, `7d`, `30d`, and `All`; bounded windows are inclusive
  local calendar days in `QueryFilter.timezone`, and `All` is the startup
  default. Windows govern Models, Daily, Hourly, Cost, Stats source mix/context
  pressure, and Behavior activity/tools/optimize/compare. Overview, the 365-day
  heatmap, sync center, zombie inventory, and Blocks keep their fixed semantics.
- Bounded all-source context pressure executes one `(source, event_at)` indexed
  range per registered source. The TUI may run those ranges concurrently, then
  reconstruct `avg_percent` by weighting each source average by
  `priced_events`; counts sum and `peak_percent` is the maximum source peak.
- Recent Blocks preserves the historical anchor chain. It reverse-probes each
  registered source through `idx_usage_event_source_event_at`, merges timestamps,
  and starts the normal block engine at the first event after the latest
  pre-cutoff adjacent gap at least as long as the block session. No qualifying
  gap falls back to the full scan. Project filters and `token_limit=max` also
  retain the full scan because their historical semantics cannot be truncated.
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
| TUI window changes on a governed panel | Invalidate governed payloads, increment generation, interrupt the old request, and reload with local-date bounds |
| TUI window changes on a fixed/lifetime panel | Update the visible window label without changing that panel's payload semantics |
| Recent Blocks finds no pre-cutoff gap | Fall back to the historical full scan and retain identical block rows |
| Recent Blocks uses project filtering or `token_limit=max` | Keep the full scan so fuzzy-project and historical-maximum semantics remain exact |
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
- Good: switching `All -> 30d` reloads Models/Stats/Behavior with inclusive
  local-date bounds while Overview totals and the 365-day heatmap remain stable.
- Good: a recent Blocks scan re-anchors after a five-hour gap and returns the
  same rows as the full engine while scanning only post-gap events.
- Base: continuous history without a five-hour gap keeps the full Blocks scan.
- Bad: starting Blocks at `now - 3d - 5h`; a block anchor can chain across that
  timestamp and change every later block boundary.
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
- TUI window tests assert Today/7d/30d inclusive local dates, `All` default and
  cleared bounds, governed-panel invalidation, and fixed-panel result matching.
- Context-pressure tests compare bounded all-source output with weighted
  per-source output and assert `idx_usage_event_source_event_at` in the plan.
- Blocks tests compare bounded/full rows across a cutoff-crossing block and
  cover gap re-anchor, active detection, and no-gap fallback. Representative
  release evidence records three-sample medians plus probe/main scanned rows.
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

For recent Blocks, the correct cutoff is data-dependent:

```text
cutoff = now - 3 days
anchor = first event after latest adjacent-event gap >= session_length
scan = events where event_at >= anchor
fallback = full history when anchor is absent
```

## Scenario: Live dashboard read cache and HTTP transfer

### 1. Scope / Trigger

- Apply this contract when changing WebState dashboard reads, diagnostics freshness, sync-job terminal hooks, web SQLite lock waits, embedded asset responses, compression, or automatic refresh routing.
- This is a web-boundary optimization. Direct query-library calls, sync writers, static export, and API response fields keep their existing semantics.

### 2. Signatures

```text
Store::open_connection_with_busy_timeout(Duration) -> Result<Connection>
Dashboard::open_with_busy_timeout(&Store, Duration) -> Result<Dashboard>
Dashboard::core_snapshot_with_diagnostics(&QueryFilter, &DiagnosticsPayload)
Dashboard::interactive_snapshot_with_diagnostics(&QueryFilter, window, &DiagnosticsPayload)
JobRegistry::register_terminal_hook(Fn() + Send + Sync + 'static)
GET /assets/<path> with If-None-Match / Accept-Encoding
GET /api/dashboard?scope=interactive&since=<date>&until=<date>
```

### 3. Contracts

- `WebState` owns one 30-second diagnostics cache shared by `/api/diagnostics` and all dashboard scopes. `Dashboard::diagnostics()` and `Dashboard::home_overview()` remain uncached cold reads.
- Cold cache fills are single-flight. Every invalidation advances a generation while holding the cache write lock; an older in-flight computation must neither return nor store its pre-invalidation payload and recomputes under the same single-flight guard.
- Completed, failed, and cancelled sync jobs invalidate diagnostics through a cheap `JobRegistry` terminal hook. `/api/diagnostics/forget` also invalidates. TTL expiry detects external file deletion that bypasses both paths.
- Web/API `Dashboard` connections use a 1500 ms `busy_timeout`; default Store connections retain 30 seconds for sync writers and migrations.
- Automatic refresh and post-sync refresh always use `scope=interactive`, including explicit `since`/`until`, then refresh secondary sections with concurrency 2. They never fall back to full scope in live mode.
- Embedded assets return `Cache-Control: no-cache` and a stable content ETag. Matching strong or weak `If-None-Match` returns `304` with an empty body. gzip/Brotli compression applies to eligible assets and JSON responses; JSON endpoints do not gain cache headers.
- The live index HTML is generated once per process because it depends only on compile-time/runtime registry metadata.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Diagnostics cache hit before 30s | Return a clone with zero filesystem stat calls |
| TTL expires | Run exactly one new cold stat pass |
| Sync terminates during a cold fill | Fence the old generation and recompute before publishing |
| External file disappears | Keep the cached value only until TTL expiry, then report missing |
| SQLite remains locked | Surface the lock error near 1500 ms and enter the existing timeout/degraded path |
| Matching asset ETag | Return `304`, ETag, `Cache-Control: no-cache`, and no body |
| `Accept-Encoding: gzip` or `br` | Compress eligible asset/API bodies and preserve decoded content |
| Custom `since`/`until` auto-refresh | Request interactive core plus independent secondary sections, never full scope |

### 5. Good/Base/Bad Cases

- Good: eight concurrent cold dashboard reads perform one diagnostics stat pass and share the payload.
- Good: a sync completes during that pass; the old generation is discarded and the waiter receives a post-sync recomputation.
- Base: `Dashboard::diagnostics()` called by a library consumer still performs a cold read on every call.
- Bad: an invalidation clears the entry, then an older cold task writes its stale payload back for another 30 seconds.
- Bad: a custom-date automatic refresh silently switches to full scope and recreates connection/DOM fan-out.

### 6. Tests Required

- Rust tests cover TTL hit/expiry, external deletion, terminal invalidation, generation fencing, single-flight concurrency, API/dashboard sharing, short busy timeout, ETag/304, compression, and stable root HTML.
- Contract tests keep full/core/interactive response shapes unchanged and retain per-section degraded behavior.
- Node tests cover semantic/panel fingerprints, context/formatter reuse, and section-local DOM writes.
- Run representative interactive benchmarks and `just ci` before completion.

### 7. Wrong vs Correct

#### Wrong

```text
invalidate() -> entry = None
old cold task finishes -> entry = stale payload
```

#### Correct

```text
invalidate() -> generation += 1; entry = None
old cold task sees generation mismatch -> discard and recompute
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
