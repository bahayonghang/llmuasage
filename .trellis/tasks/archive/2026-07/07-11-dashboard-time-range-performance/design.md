# Design

## Scope And Boundary

This is one cross-layer performance task because the user-visible budget spans
the browser, web API, query planner, and SQLite read model. Splitting these into
independent tasks would allow each layer to pass locally while the range click
still misses the end-to-end budget.

Primary files:

- `src/web/assets/app.js`
- `src/web/assets/data/fetch.js`
- `src/web/assets/data/derive.js`
- affected render modules for incremental panel updates
- `src/web/mod.rs`
- `src/query/mod.rs`
- `src/query/explorer.rs`
- `src/store/migrations.rs` only if post-routing query plans justify indexes
- focused tests plus a browser/HTTP performance harness

Parser, sync ingestion, pricing, and visual redesign remain outside the task.

## Current Critical Path

```text
range click
  -> mutate state (button is not visually synced yet)
  -> GET /api/dashboard?scope=core
       -> one blocking SQLite task
       -> overview + sync center + 4 trend windows
       -> models + sources + projects + costs
       -> all 7,382 cursor records + diagnostics
  -> parse ~1.7 MB and render every dashboard section
  -> GET /api/dashboard (background)
       -> repeat the entire core snapshot
       -> concurrently run activity/tools/optimize/explorer/compare
       -> wait for the slowest section
  -> parse ~1.7 MB and render every section again
```

The current path is asynchronous at the Tokio scheduling level, but it performs
too much duplicated synchronous SQLite work. Timeout only drops the await; the
`spawn_blocking` query continues.

## Ranked Root Causes

1. **Oversized core contract.** Full cursor detail is 98%+ of the core payload
   while the web UI needs only a count. This adds query, serialization,
   transfer, JSON parsing, allocation, and repeated context normalization.
2. **Duplicate core work.** The background full request recomputes core after
   the fast request. This doubles the critical query family on every cold range
   click and competes with secondary work.
3. **Wrong read model for default Explorer.** Source/model/project cost, token,
   and event-count aggregates can use `usage_bucket_30m`, but currently scan
   the 523k-row event table for both rows and series.
4. **Unseekable time-only fact queries.** Composite indexes begin with source,
   so global range filters scan event/turn/tool tables. This primarily affects
   fallback Explorer and behavior sections.
5. **Latest-wins is logical, not physical.** Generation checks prevent stale
   rendering, but `fetch` has no `AbortController`, and server timeouts/client
   disconnects do not interrupt SQLite. Rapid switching can leave obsolete
   work consuming the four query permits.

The July 11 Explorer renderer change is not a leading hypothesis: it bounds
visible series to five and details to 80 rows. It may affect render cost, but it
does not explain the measured API latency or 1.69 MB cursor payload.

## Proposed Data Flow

```text
range click
  -> synchronously select button + mark range refresh state
  -> abort previous range generation
  -> GET lean interactive range payload
       -> selected trend only
       -> overview/rankings/sync status/diagnostics
       -> health summary, not cursor rows
  -> render only range-dependent critical sections
  -> schedule secondary sections with the same generation + AbortSignal
       -> activity/tools/optimize/explorer/compare independently
       -> publish each completed section if generation is still current
       -> keep explicit stale/loading/degraded metadata per section
```

## API Contracts

Keep the existing full `/api/dashboard` and static snapshot/export shape for
compatibility. Add an explicit interactive projection, preferably
`scope=interactive`, rather than silently changing full snapshot fields.

Interactive payload:

- `overview`
- `trends` for the selected range only
- `models`, `sources`, `projects`, `costs`
- `sync_command_center`
- `health_summary` with integration rows, recent failure summary, and
  `cursor_count`; no cursor-detail array
- `diagnostics`
- response metadata identifying normalized filter/range and generated time

Secondary data should use existing section endpoints initially. This allows
`Promise.allSettled`-style independent completion and avoids a monolithic
secondary response waiting for Explorer. If request overhead becomes material,
an additive secondary batch endpoint may be added, but it must not recompute
interactive data and must preserve per-section status.

## Frontend Orchestration

- Update pressed/selected state before the first `await`.
- Maintain one generation-scoped `AbortController` for range refresh.
- Extend `loadJson`/`loadLiveJson` to accept `AbortSignal` without treating
  intentional aborts as dashboard errors.
- Bound the live cache with LRU/size limits; preserve normalized keys and the
  10-second freshness behavior unless measurement recommends a smaller TTL.
- Do not clear the in-flight map without aborting its requests.
- Merge critical and secondary payloads by generation and filter key, not only
  by mutable global state.
- Render critical and completed secondary sections incrementally. Avoid running
  `buildContext` and every renderer for a one-section completion when a smaller
  render boundary exists.
- Use idle/visibility scheduling for off-screen secondary panels only after the
  critical path is correct; never hide stale state.

## Query Strategy

Add `ExplorerStrategy::Bucket` (name may vary) when all requested semantics are
available in `usage_bucket_30m`:

- dimensions: source, model, project;
- metrics: attributed cost, total tokens, calls/event count;
- filters: source, model, project, since/until, timezone;
- no session/tool/tool-kind/is-tool/token-type requirement.

Keep event/turn/attribution strategies for unsupported dimensions and filters.
Add equivalence tests that seed the same facts into event and bucket tables and
compare totals, rows, Other handling, series buckets, and time-zone boundaries.

The user approved a forward-only migration when evidence justifies it. After
aggregate routing, capture `EXPLAIN QUERY PLAN` for remaining common global
time-filter paths. Candidate indexes are:

- `usage_event(event_at)`
- `usage_turn(started_at)`
- `usage_tool_call(occurred_at)`

Keep an index only if the plan changes from scan to search and the measured
read gain outweighs bootstrap/migration time, database size growth, and sync
write amplification. Approval does not require adding all candidate indexes.

## Real Cancellation

Browser abort alone does not stop an already running `spawn_blocking` query.
Use the existing `tokio-util` cancellation facilities plus rusqlite 0.40.1's
`Connection::get_interrupt_handle()`:

1. Open the Dashboard connection inside the blocking task.
2. Send its `InterruptHandle` to the async wrapper before executing the query.
3. Hold an async-side guard that calls `interrupt()` on timeout or future drop.
4. If the receiver was already dropped, interrupt/skip the query before work.
5. Await or otherwise account for the blocking task so query permits are not
   silently leaked after timeout.

Map intentional SQLite interruption to an internal cancelled outcome, not a
user-visible configuration error. Add a deterministic slow-query test proving
the blocking statement stops after timeout/cancellation.

## Async And Concurrency Trade-Offs

- Keep SQLite on `spawn_blocking`; moving synchronous rusqlite calls directly
  into async tasks would block Tokio workers.
- Do not increase `WEB_DASHBOARD_QUERY_PERMITS` above four without profiling.
- Prefer removing duplicate queries and interrupting obsolete work over adding
  concurrency.
- Secondary section concurrency should be bounded (recommended two) and may
  prioritize the visible section. More parallel reads against a 1.1 GB SQLite
  file can increase tail latency.

## Compatibility And Rollback

- Full dashboard and export payloads remain available.
- Snapshot mode stays synchronous and complete because it has no live range
  interaction.
- Existing degraded/unsupported/no-data semantics remain per section.
- Roll back in layers: cancellation guard, secondary scheduler, interactive
  projection, bucket strategy, then optional indexes. Each layer has its own
  targeted tests and benchmark checkpoint.
- If bucket equivalence cannot be proven for a query shape, retain the existing
  strategy for that shape rather than approximating results.

## Observability

Add structured timings for dashboard scope and section name with query,
serialization, payload-size, cancelled, and semaphore-wait dimensions. Keep
logs debug-level or opt-in so normal CLI output stays quiet. The benchmark must
report median/p95 rather than a single warm-cache sample.
