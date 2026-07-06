# Design

## Architecture And Boundaries

This task should stay within the live dashboard read path:

- `src/web/assets/app.js`
- `src/web/assets/data/fetch.js`
- `src/web/assets/data/derive.js` only if panel state needs normalized stale/loading metadata
- `src/web/mod.rs`
- `src/query/mod.rs`
- `src/query/explorer.rs`
- focused tests in `src/web/mod.rs` and query tests if backend cache behavior is added

Do not change parser, sync writer, pricing, source registry, or SQLite import semantics.

## Current Data Flow

On a time-range button click:

1. `setupTrendSegments` updates `state.trendWindow`.
2. The handler clears `state.filters.since` and `state.filters.until`.
3. It calls `reloadDashboard(state)`.
4. `reloadDashboard` calls `loadDashboardData`, then re-renders the full dashboard.
5. `loadDashboardData` calls `/api/dashboard`, then separately calls `/api/explorer`.
6. `/api/dashboard` builds `core_snapshot`, then joins Activity, Tools, Optimize, Explorer, and Compare.

This makes a trend-range click pay for all panels. The legacy `/api/trends` endpoint is fast, so the delay is caused by orchestration and heavy sections rather than the trend query alone.

## Proposed Strategy

Use a layered responsiveness model instead of one monolithic cache.

### 1. Frontend Request Cache And In-Flight Coalescing

Add a small live-mode request cache in `fetch.js` or adjacent data code:

- key: normalized endpoint plus normalized query string;
- value: resolved payload plus `receivedAt`;
- in-flight map: key to promise;
- TTL: short, on the order of 5-15 seconds for dashboard/query payloads;
- invalidation: clear relevant cache after a sync job completes, after manual refresh/reset, and when source/model/project/date filters change.

This handles rapid repeated clicks and auto-refresh overlap without changing API contracts.

### 2. Stop Duplicate Explorer Loading

If `/api/dashboard` returns `explorer`, `loadDashboardData` should not immediately call `loadExplorer(state)` again for the same default Explorer query.

Only call `/api/explorer` when:

- the user changes Explorer-specific controls;
- `/api/dashboard` did not include an Explorer payload;
- the dashboard Explorer payload is degraded and the UI explicitly retries;
- the request key differs from the default Explorer query embedded in the snapshot.

### 3. Split Critical And Secondary Refresh

For range-button changes, update first-screen sections first:

- overview/KPIs;
- trends for the requested range;
- model/source/project/cost rankings if they remain part of the visible first-screen contract;
- sync status only if it can be refreshed cheaply or from a separate status endpoint.

Heavy secondary sections should use stale-while-refresh:

- Activity;
- Tools;
- Optimize;
- Compare;
- Explorer when not currently in the active viewport or when unchanged.

The user approved this stale-while-refresh path. The page may render existing secondary data with a subtle refreshing state, then replace it when the background request finishes. The copy must not imply stale data is current if the filter has already changed.

### 4. Backend Snapshot Shape

Prefer a minimal additive API/query option rather than breaking existing consumers.

Candidate shape:

- keep `/api/dashboard` backwards compatible;
- add query params such as `scope=core|full` or `include_behavior=false&include_explorer=false`;
- or add `/api/dashboard/core` for first-screen payloads.

Recommended default for implementation: add a scoped dashboard path or parameter that returns the critical payload only, then let existing `/api/dashboard` keep the full snapshot/export compatibility contract.

### 5. Backend Cache

Only add backend caching after removing duplicate Explorer and splitting critical/secondary refresh. If backend caching is still needed, use an in-process, bounded cache:

- cache key: normalized `QueryFilter`, requested scope, Explorer query options, and a database freshness token;
- freshness token candidates: max successful sync `finished_at`, max `source_sync_status.updated_at`, and active job state;
- TTL: short enough to avoid stale confusion in live local use;
- invalidation: sync job completion, manual sync start/finish, and process-local cache clear.

Do not persist dashboard caches into SQLite unless evidence shows in-process cache is insufficient.

### 6. Query Optimization

Review slow all-range paths after orchestration fixes:

- avoid computing four legacy trend windows when only one range is needed for live mode;
- avoid behavior sections in first-screen refresh;
- consider covering indexes for common unfiltered all-range grouping only if query plans prove scans are the bottleneck;
- preserve backend aggregation for Explorer.

## Compatibility

- Existing `/api/dashboard` and snapshot export consumers should keep working.
- Existing docs that describe `/api/dashboard` as the primary snapshot seam remain true.
- Explorer remains additive and backend-aggregated.
- Degraded states remain explicit for behavior/query sections.
- Sync job actions and active job state must remain live enough for user trust.

## Trade-Offs

Fast first-screen update with stale-while-refresh gives the user immediate feedback but requires clear loading/stale state. The user accepted this trade-off. Atomic all-panel update is simpler conceptually, but repeats the current pain when heavy sections dominate.

In-memory caching is easy to invalidate and safe for local live use. SQLite-persisted cache would survive restarts but adds schema and migration risk for a dashboard latency issue that is likely request orchestration first.

## Validation

- Add asset tests that prove range switching uses the fast/scoped path and does not double-fetch Explorer.
- Add web handler tests for scoped dashboard payload behavior if a scope parameter/route is introduced.
- Add query tests for any new cache key or freshness-token logic.
- Measure endpoint latency before and after on the same local data shape.
- Browser-check range switching at desktop and mobile widths for no broken loading state or overlap.
