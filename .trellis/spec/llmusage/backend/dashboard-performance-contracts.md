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
DashboardQuerySupervisor::supervise(query_id, section, blocking_task)
    -> hard-deadline response + background JoinHandle settlement
GET /api/diagnostics
    -> DiagnosticsPayload fields + dashboard_query_inflight + timed_out_tasks
       + orphaned_tasks + orphan_duration_ms
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
- `sync_command_center` source rows may include four parse-issue counters
  (`malformed_lines`, `oversized_lines`, `skipped_lines`,
  `accounting_anomaly_lines`) with serde defaults. Parse-issue samples stay
  out of interactive dashboard JSON.
- Interactive `health` is a summary with `cursor_count`; it must not serialize
  the full cursor array. Full and core contracts keep their existing shapes.
- A range click updates selected/loading state before its first `await`, aborts
  the previous generation, and accepts results only when both generation and
  stable filters still match.
- Secondary `activity`, `tools`, `optimize`, `explorer`, and `compare` requests
  run independently with concurrency `2`. They may update only their own
  section and must retain stale/loading metadata until all current-generation
  sections settle.
- Live bootstrap requests `scope=interactive` with legacy fan-out disabled. A failed core request
  issues no overview/trends/models endpoint fallback; full and legacy APIs remain available to
  explicit compatibility callers, and static snapshots still load their complete payload.
- Core loading renders before its first await, becomes slow after 2 seconds, and aborts at 6
  seconds. Retry creates a new generation/controller, so stale responses cannot replace it.
- After core paint, the five secondary sections expose exact settled `0..5` progress. Fulfilled and
  degraded results both settle once; complete or error states stop loading motion.
- Live response caching keeps normalized request keys for 10 seconds, is
  capped at 32 entries, and aborts in-flight requests during invalidation.
- Server-side dashboard work remains on `spawn_blocking`, holds one of four
  query permits for the blocking task lifetime, and publishes an SQLite
  `InterruptHandle`. A timeout interrupts when possible and returns the
  structured timeout at the configured hard deadline; it does not await the
  blocking task in the request future. `DashboardQuerySupervisor` takes the
  `JoinHandle`, awaits it in a background Tokio task, and the blocking closure
  retains its permit until it actually exits.
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
  default. Windows govern Models, Daily, Hourly, Cost, Stats context
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
- Debug timing fields are `query_id`, `section`, `semaphore_wait_ms`, `query_ms`,
  and `cancelled`; API serialization adds `endpoint`, `serialization_ms`, and
  `payload_bytes`. A timed-out blocking task emits `Dashboard query orphan
  settled` with the same `query_id` and `section`, plus `orphan_duration_ms`
  and a bounded join-outcome label after the task really ends.
- `/api/diagnostics` appends live `dashboard_query_inflight`,
  `timed_out_tasks`, `orphaned_tasks`, and nullable `orphan_duration_ms` fields
  after loading the existing diagnostics payload. These values never enter the
  30-second diagnostics cache, and dashboard archive payloads keep their
  existing shape.

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
| Query timeout before/after handle publication | Interrupt when possible, transfer the JoinHandle to the supervisor, return the structured timeout at the hard deadline, and retain the permit until background settlement |
| Timed-out task ignores SQLite interrupt | Request still returns within timeout +100 ms; inflight/orphan metrics remain nonzero until the closure really exits |
| Supervisor task settles | Emit the matching query-ID settled event, decrement inflight/orphan counts, and record orphan duration without user dimensions |
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
- Good: a timed-out non-cooperative closure returns at the hard deadline, keeps
  its permit while running, then produces a query-ID-matched settled event.
- Bad: `drop(task)` detaches an unobservable query, or `task.await` in the
  request future turns the configured timeout into an unbounded response.
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
- Rust cancellation tests force both an interruptible SQLite statement and an
  interrupt-ignoring closure. They assert the hard response boundary, permit
  ownership through real completion, supervisor counters/duration, matching
  settled lifecycle, and the additive live diagnostics fields.
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

```text
timeout -> interrupt -> drop(blocking JoinHandle) -> return
```

This preserves response latency but loses the completion boundary needed for
permit diagnostics and paired performance evidence. Awaiting the handle in the
request future is also wrong because non-cooperative work can exceed the public
deadline.

#### Correct

```text
timeout -> interrupt -> supervisor owns JoinHandle -> return
background await -> same query_id settled log -> permit/inflight released
```

The supervisor exposes only bounded lifecycle metrics; it never persists query
inputs or response data.

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

## Scenario: Behavior exact reads under bounded deadlines

### 1. Scope / Trigger

- Apply this contract when changing Activity, Tools, Optimize, or Compare
  queries, their loopback HTTP handlers, Behavior deadlines, or indexes used by
  normalized behavior facts.
- Behavior output stays exact. Performance work must not approximate distinct
  turns/sessions, sibling attribution, costs, filters, findings, or model
  comparison fields.

### 2. Signatures

```text
GET /api/activity|tools|optimize|compare?range=<1d|7d|30d|all>&...
Dashboard::activity_breakdown(&QueryFilter) -> Result<ActivityPayload>
Dashboard::tool_breakdown(&QueryFilter) -> Result<ToolsPayload>
Dashboard::optimize(&QueryFilter) -> Result<OptimizePayload>
Dashboard::model_compare(&QueryFilter, model_a, model_b)
    -> Result<ModelComparePayload>
WEB_BEHAVIOR_API_TIMEOUT = 3 seconds
WEB_API_TIMEOUT = 5 seconds
schema v18 = Behavior range/attribution indexes
schema v19 = idx_usage_event_activity_cost(event_key, cost_with_cache_usd)
```

Schema v18 creates `usage_event(event_at)`, `usage_turn(started_at)`,
`usage_turn(session_id)`, `usage_tool_call(event_key)`,
`usage_tool_call(occurred_at)`, and `usage_tool_call(model, occurred_at)`, and
recreates `idx_usage_turn_event_key_expr` for already-versioned drifted
databases.

Schema v19 adds only `idx_usage_event_activity_cost` on
`usage_event(event_key, cost_with_cache_usd)`. It covers Activity's full
event-cost projection without changing the query text or Rust reducer.

### 3. Contracts

- Activity streams persisted event costs and filtered turns, then aggregates
  exact category fields in Rust. A turn without a matching event contributes
  zero cost, and row ordering remains cost, tokens, turns, category.
- The v19 Activity index is schema-only. It must not change filters, NULL and
  missing-event cost handling, floating-point accumulation order, cache,
  concurrency, frontend behavior, PERF-002 settlement, or the three-second
  deadline. A fixed synthetic `SyncRunWriter` benchmark must keep the indexed
  median at or below `1.10` times the no-index median.
- Tools counts filtered siblings first, attributes each linked event equally,
  excludes orphan tools, and preserves the deliberate asymmetry: tool rows use
  the tool filter while `(non-tool)` rows use the event filter. Bounded requests
  load range-matching events plus missing filtered-tool event keys; `all` may
  use the sequential full-event projection.
- Optimize probes support with `EXISTS`, counts read/edit calls without joining
  every event, joins only edit rows for savings, selects the top session before
  its cost lookup, and leaves the other detector thresholds unchanged.
- Compare aggregates both selected models in one bucket query, one turn query,
  one tool query, and one category query while preserving missing-model,
  low-sample, metric, category, and working-style payloads.
- The browser keeps Behavior concurrency at two and each section settles
  independently. The Behavior deadline is three seconds; the general API
  deadline remains five seconds. PERF-002 interrupts timed-out work, returns at
  the hard deadline, and transfers the JoinHandle to the background supervisor;
  the blocking closure keeps its query permit until it actually exits.
- On representative data, every `1d` three-sample median must be below one
  second and every `all` sample must finish below three seconds without timeout
  degradation.
- Before a real database first bootstraps to a newer index schema, create and
  retain a separate SQLite online backup. Verify it read-only with the old
  schema version, `PRAGMA integrity_check=ok`, matching aggregate row counts,
  and a recorded restore path. A migrated profiling copy is not that backup.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| No matching normalized facts | Existing explicit `no_data` payload |
| Valid low-sample comparison | `low_sample`, not degraded |
| Tool row has no matching event | Exclude it from attributed output |
| Event has no filtered tool sibling | Include it in `(non-tool)` under the event filter |
| Query exceeds three seconds | Section-local degraded payload; interrupt, return at the hard deadline, and supervise background settlement while retaining the permit |
| Another Behavior section fails | Other sections continue and settle independently |
| v17 lacks the historical expression index | v18 creates it with all final indexes |
| Schema v18 opens in a v19 binary | Apply only the Activity covering-index migration and advance to v19 |
| v19 sync benchmark ratio exceeds `1.10` | Block the migration release; do not enter D2 automatically |
| Pre-migration backup is missing or fails integrity | Do not bootstrap the real database |
| Older binary opens schema v19 | Reject as newer schema; restore the retained pre-v19 backup for rollback |

### 5. Good/Base/Bad Cases

- Good: `range=1d` uses bounded projections, returns exact normalized payloads,
  and has a sub-second three-sample median.
- Base: `range=all` performs exact sequential aggregation and completes below
  the three-second Behavior deadline.
- Good: a model/date filter includes a linked filtered tool even when the
  linked event does not match the event filter, while non-tool rows still obey
  that event filter.
- Good: a v18 database upgrades to v19 and Activity serializes byte-for-byte
  identically for missing events, NULL costs, zero edit turns, category ties,
  and source/model/project/date/no-data filters.
- Bad: raising or removing the old one-second timeout without changing the
  query shape.
- Bad: migrating the only v17 copy during profiling and then calling that v18
  database a rollback backup.
- Bad: rewriting Activity as SQL `SUM`/`GROUP BY`, changing the reducer, or
  accepting an indexed sync median more than 10% slower as part of v19.

### 6. Tests Required

- Migration tests start from a v17-shaped database with the expression index
  removed, then assert schema v18 and all seven indexes; fresh bootstrap must
  expose the same set. Keep this test isolated with `MIGRATIONS[..18]`.
- The v19 migration test upgrades a v18 schema and bootstraps a fresh schema;
  both must reach version 19, expose the exact two index columns in order, and
  show the full event-cost projection using the covering index.
- Activity and Tools compare complete serialized results against test-only
  legacy SQL across empty, filtered, multi-tool, non-tool, and orphan cases.
- Activity additionally compares serialized bytes before and after creating
  the v19 index across missing-event, nullable-cost, zero-edit, category-tie,
  source/model/project/date, and no-data cases.
- Optimize and Compare compare complete serialized results against legacy
  implementations for positive, negative, filtered, missing-model,
  low-sample, and normalized cases.
- Query-plan tests assert every v18 index is usable; trace tests assert selected
  Compare models share each query family.
- Web tests prove Behavior may complete after the former one-second deadline,
  while the general five-second deadline and supervised cancellation/permit
  contracts remain intact.
- Representative validation records three `1d` and three `all` samples for
  each section, a concurrency-two round, and a real browser DOM check with no
  loading or timeout text.
- Run the ignored acceptance benchmark explicitly with one test thread. It
  must alternate indexed/no-index order over fixed 4,000-event shards, compare
  seven-sample medians, and hard-fail above ratio `1.10`.
- Run `python scripts/ci-rust.py` and `just ci` before completion.

### 7. Wrong vs Correct

#### Wrong

```sql
SELECT ...
FROM usage_tool_call tc
JOIN usage_event e ON e.event_key = tc.event_key
GROUP BY ...;
```

This repeats random event lookups and builds several temporary group/distinct
B-trees across the full history; a larger deadline only hides that cost.

#### Correct

```text
filtered tool projection -> sibling counts
bounded event projection + missing linked event keys -> attribution map
single Rust reducer -> exact tool/non-tool rows, distinct counts, sort, top 50
```

Optimize should likewise reduce before joining: count tool kinds first, join
only edit rows for savings, and choose the top session before its cost lookup.

For Activity's event-cost projection, do not change the reducer to chase the
deadline:

```sql
-- Wrong: changes floating-point aggregation order and result semantics.
SELECT t.category, SUM(e.cost_with_cache_usd)
FROM usage_turn t LEFT JOIN usage_event e ON ...
GROUP BY t.category;

-- Correct schema-only optimization: preserves the existing row stream.
CREATE INDEX IF NOT EXISTS idx_usage_event_activity_cost
    ON usage_event(event_key, cost_with_cache_usd);
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
- Web query supervisor counters are attached to `/api/diagnostics` after the
  cached filesystem payload is obtained, so cache hits never freeze live
  inflight/orphan state. Dashboard archive fields do not gain these Web-only
  counters.
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

The loopback-only HTTP route has two additive response modes:

```text
GET /api/home_overview?<QueryFilter fields>
    -> HomeOverviewPayload (unchanged full response)
GET /api/home_overview?compact=true&<QueryFilter fields>
    -> { summary, by_platform }
Dashboard::home_overview_compact(&QueryFilter) -> HomeOverviewSnapshot
```

Only the existing truthy query values accepted by `parse_bool_query` (`1`,
`true`, `yes`, or `on`, case-insensitive) select compact mode. Omitted, false,
or unknown values retain the full response. Compact mode applies the same
source, model, project, inclusive date, and timezone filters as the full mode;
its two fields must preserve exact integer, key, and structural semantics from
the full response projection. Floating-point fields are equivalent when their
absolute delta is at most the shared test tolerance `EPSILON = 1e-9`; index
scan ordering may change only floating-point accumulation order within that
tolerance.
It reads and aggregates the shared event stream but skips daily series,
run-state, and archive diagnostics work. Static snapshots use this same compact
query path. The route remains absent from the public read-only router in both
modes, and query failures retain the existing structured 500 response.

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

Schema v20 adds exactly one compact-query index:

```sql
CREATE INDEX idx_usage_event_home_compact_cover
ON usage_event(
    event_at, source, model, project_hash,
    COALESCE(NULLIF(session_id, ''), NULLIF(source_path_hash, ''), event_key),
    input_tokens, cache_creation_tokens, cache_read_tokens, total_tokens,
    cost_with_cache_usd
);
```

The all-range and date-range compact projection must be covering under this
index. Do not add a second identity-first index or a table-order cost rescan:
the former duplicates about one index footprint without clearing the end-to-end
budget, while the latter violates the 400 ms representative-data budget solely
to recover byte-level floating-point accumulation order.

Archive diagnostics may aggregate `usage_bucket_30m` only for sources with
missing source files; when all files are live, the bucket scan is skipped.
This is a query-path optimization with no schema or migration change, and it
must retain protected-event counts and archive payload fields exactly.

Validation requires the focused 80 ms test to pass three consecutive times in
debug and release, exact cross-day/session/filter integer and structural
coverage, floating-point deltas within `EPSILON = 1e-9`, and a read-only or
online-backup profile for representative databases. No process cache, warm-up
query, delayed work, platform exception, or threshold relaxation is allowed.

| HTTP condition | Required result |
| --- | --- |
| `compact=true` with any stable `QueryFilter` fields | Exactly `summary` and `by_platform`; integer/key/structure exact and `f64` delta ≤ `1e-9` versus the full projection |
| `compact` omitted, false, or unknown | Existing full payload and byte/shape semantics |
| `compact=true` on the public read-only listener | 404; the loopback-only route is not mounted |
| SQLite/query failure in either mode | Existing structured `internal_error` response |

Tests must compare compact and full projections through both the query API and
the real loopback HTTP boundary, cover combined source/model/project/date/IANA
filters, assert the default full-only keys remain present, assert the compact
response has only two keys, verify the browser requests `compact=true`, and
verify schema v20 upgrades/fresh installs plus the covering plans.

## Scenario: Browser IANA timezone queries

### 1. Scope / Trigger

- Apply this contract when a Dashboard HTTP query, browser request builder, or
  date-grouped query changes timezone handling.
- This is additive to the existing UTC, local, and fixed-offset behavior.

### 2. Signatures

```text
GET /api/<dashboard-endpoint>?timezone=<IANA name|UTC|local|fixed offset>
ReportTimezone::Iana(chrono_tz::Tz)
buildFilterQuery(state, options) -> query string
```

### 3. Contracts

- HTTP parsing order is UTC/`Z`, `local`, fixed offset, IANA name, then the
  legacy `Local` fallback for an unknown or omitted value.
- Live browser requests add
  `Intl.DateTimeFormat().resolvedOptions().timeZone` unless `filters.timezone`
  already supplies a value. Static snapshots do not require a live timezone.
- IANA date bounds, labels, heatmaps, and daily groupings use the historical
  offset for each instant, including daylight-saving transitions.
- Heatmap zero-fill windows end at an explicit `QueryFilter.until`; only an
  unbounded request ends at the current local date. This keeps historical
  custom ranges aligned with their rendered calendar cells.
- Existing exports from `data/fetch.js` and existing UTC/local/fixed-offset SQL
  behavior remain unchanged.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Canonical IANA name | Use `ResolvedZone::Iana` with historical DST rules |
| Explicit browser timezone filter | Preserve it instead of auto-detecting |
| `UTC`, `Z`, `local`, or fixed offset | Preserve the existing parse path |
| Unknown or omitted HTTP value | Fall back to `Local` without a request error |
| Browser cannot resolve a timezone | Omit the automatic parameter |

### 5. Good/Base/Bad Cases

- Good: `timezone=America/New_York` groups winter and summer instants with the
  offsets active on those dates.
- Base: `timezone=UTC+8` produces the same date bounds as before.
- Bad: treating an IANA zone as one current fixed offset, or replacing an
  explicit timezone with the browser default.

### 6. Tests Required

- Rust parser tests cover Shanghai, New York, UTC/`Z`, local, fixed offset, and
  unknown-name fallback.
- At least one HTTP date-grouping endpoint proves a no-DST boundary and a DST
  boundary.
- Node request tests prove automatic IANA propagation and explicit override.
- Run `just ci` before completion.

### 7. Wrong vs Correct

#### Wrong

```text
IANA name -> current numeric offset -> all historical dates
```

#### Correct

```text
IANA name -> ReportTimezone::Iana -> ResolvedZone::Iana -> per-instant offset
```

## Scenario: Session analytics dashboard reads

### 1. Scope / Trigger

- Apply this contract when changing Top Sessions, the 7x24 hour grid, Logs
  session/detail filtering, analytics CSV export, or their browser loading
  lifecycle.

### 2. Signatures

```text
GET /api/sessions?sort=<tokens|duration|cost>&limit=<1..50>&<QueryFilter>
GET /api/hour_of_week?<QueryFilter>
GET /api/logs?session=<id|canonical id|label>&event_key=<exact key>
Dashboard::top_sessions(&TopSessionsQuery) -> Vec<TopSessionRow>
Dashboard::hour_of_week(&QueryFilter) -> HourOfWeekPayload
```

### 3. Contracts

- Top Sessions applies the complete `QueryFilter`, clamps the limit to 50,
  sorts on the server, and uses canonical session id as the stable tiebreaker.
- Duration means active minutes: sum only positive adjacent-event gaps of at
  most 30 minutes. Duration ranking must compute every filtered candidate
  before truncation; a span-ranked `3 * limit` preselection is not exact.
- Event times for duration ranking are loaded in one ordered batch and reduced
  by canonical session id. Do not add one query per candidate.
- `hour_of_week` returns a zero-filled 7x24 grid. Each source bucket is
  converted through `ResolvedZone` before folding, so DST fallback instants may
  contribute to the same local cell.
- Logs `session` accepts an exact source session id, an exact canonical session
  id, or a case-insensitive label substring. `event_key` selects zero-or-one
  record detail mode, includes raw JSON when available, and ignores cursor,
  page size, total counting, and the page-wide raw flag.
- Top Sessions and hour grid participate in the shared generation-guarded
  secondary lifecycle. Logs additionally fences responses by filter signature;
  reset clears loading state before the replacement request starts.
- CSV export uses the six visible summary metrics, localized labels, UTF-8 BOM,
  and prefixes cells matching `^[=+\\-@\\t\\r\\n]` with a single quote before
  standard CSV quoting.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Equal primary sort values | Canonical session id ascending decides order |
| Long idle spans outrank active span in rough order | Exact active-duration order still wins |
| DST fallback repeats a local hour | Both UTC buckets fold into the same local cell |
| Logs detail URL retains pagination parameters | Return at most one record and no next cursor/total |
| Global filters change while Logs is loading | Stale response is discarded and replacement load proceeds |
| Old snapshot omits new keys | New panels render an empty/degraded state without throwing |
| CSV cell starts with a formula trigger | Export it as inert text |

### 5. Good/Base/Bad Cases

- Good: a canonical Top Session row opens Logs and the server returns only that
  session while a rapid range change discards the older response.
- Base: token and cost rankings aggregate filtered events and return at most the
  requested limit with stable ordering.
- Bad: select `3 * limit` sessions by wall-clock span and then call that result
  the active-duration Top N, or issue an unguarded panel-specific fetch.

### 6. Tests Required

- Rust integration tests cover empty data, all filters, all three sorts and
  equal-value tiebreaks, limit clamping, an idle-span duration counterexample,
  canonical Top-to-Logs matching, detail-mode precedence, UTC/IANA folding,
  and a DST fallback repeated hour.
- Real TCP tests prove `/api/sessions` and `/api/hour_of_week` work on loopback
  and remain absent from the public router.
- Node tests cover generation/filter-signature stale rejection, sorting reload,
  old snapshots, the six localized CSV metrics, BOM, quoting, and formula
  injection protection.
- Representative warm timings remain within 400 ms per interactive endpoint,
  128 KiB per response, and 30 ms per Logs page. Run `just ci` before archive.

### 7. Wrong vs Correct

#### Wrong

```text
ORDER BY wall_clock_span DESC LIMIT 3 * N -> calculate active minutes -> LIMIT N
```

#### Correct

```text
aggregate all filtered sessions -> batch-load ordered event times
-> calculate active minutes for every candidate -> stable sort -> LIMIT N
```
