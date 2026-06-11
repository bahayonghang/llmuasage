# Optimize serve dashboard time range switching

## Goal

Make the `llmusage serve` dashboard respond quickly when the user switches the time range buttons (`1d` / `7d` / `30d` / `all`) without changing local-first data semantics or asking the browser to aggregate raw usage rows.

## User Value

The range selector is part of the first-screen workflow. A click should feel like changing a dashboard view, not like reloading the entire application for 10 seconds.

## Confirmed Facts

- The user reports that clicking the highlighted time-range buttons in the live `llmusage serve` page can stall for roughly 10 seconds.
- The current frontend time-range handler in `src/web/assets/app.js` changes `state.trendWindow`, clears explicit `since` / `until`, then calls `reloadDashboard(state)` and re-renders the full dashboard.
- `reloadDashboard(state)` calls `loadDashboardData(state)`, then `renderDashboard(state.rawData)`, so a range click reloads and re-renders all panels, not only the trend/KPI sections.
- `loadDashboardData(state)` first calls `/api/dashboard`, then separately calls `/api/explorer` in live mode.
- The live `/api/dashboard` handler builds a resilient snapshot: `core_snapshot` first, then Activity, Tools, Optimize, Explorer, and Compare are joined as behavior/query sections.
- `Dashboard::core_snapshot` currently computes overview, sync command center, four legacy trend windows, model/source/project/cost rankings, health, and diagnostics for every request.
- The architecture docs say `/api/dashboard` is the primary local SQLite snapshot seam and Explorer remains backend-aggregated; the frontend must not pivot raw transcript rows.
- The dashboard docs say core `/api/dashboard` data should remain responsive even when Activity, Tools, Optimize, Explorer, or Compare is degraded.
- Local measurement against the user's live server on `127.0.0.1:37422` before it stopped responding:
  - `/api/dashboard?window=day`: about 1.09 MB, 1.3-1.5 s.
  - `/api/dashboard?window=week`: about 1.10 MB, 1.1 s.
  - `/api/dashboard?window=month`: about 1.10 MB, 1.3 s.
  - `/api/dashboard?window=all`: about 1.15 MB, 1.8 s.
  - `/api/trends?window=day|week|month|all`: 3-9 ms.
  - `/api/explorer?range=all&granularity=day&metric=attributed_cost_usd&group_by=source&limit=8&include_other=true`: about 25 KB, 1.1 s.
- This evidence points to combined latency from full snapshot reload, duplicate/default Explorer loading, behavior/query panel work, large payloads, and full DOM re-render. The trend-only endpoint itself is fast.

## Requirements

- Preserve the existing `/api/dashboard` and `/api/explorer` local SQLite aggregation model; do not move raw event pivoting to the browser.
- Make quick time-range switching update the visible first-screen usage context quickly, especially KPI and trend sections.
- Avoid recomputing and re-rendering panels whose data did not change meaningfully for a quick range switch, or load them in a deferred/degraded path.
- Add a bounded cache or request orchestration layer where it removes real repeated work:
  - reuse identical in-flight requests;
  - reuse recent responses for the same normalized filter/range key;
  - keep heavy secondary sections stale while refreshing critical sections when appropriate.
- Keep filter URL synchronization correct for refresh/share-local use.
- Keep sync status visible and accurate; sync/job actions must not be served from stale cache when they need current job state.
- Preserve explicit degraded-state behavior for slow behavior/query panels.
- Add instrumentation or tests that make the previous "range switch reloads everything" behavior visible enough to prevent regression.
- Keep changes scoped to `llmusage serve` dashboard performance, query orchestration, and focused tests/docs.

## Acceptance Criteria

- [ ] Switching range buttons avoids a full all-panel blocking reload for the first-screen experience.
- [ ] For the measured local data shape, a range click shows updated KPI/trend content in under 500 ms when cached or under 1.5 s on a cold request; heavy secondary panels may continue refreshing after that.
- [ ] The frontend does not issue a second default `/api/explorer` request immediately after consuming an Explorer payload already returned by `/api/dashboard`.
- [ ] Identical rapid clicks or auto-refresh overlap are coalesced instead of stacking duplicate backend work.
- [ ] Sync/job state is not incorrectly hidden behind stale dashboard cache.
- [ ] Behavior sections still surface `degraded` / `unsupported` / `no_data` states instead of silently displaying stale-as-current data.
- [ ] Targeted Rust and web asset tests cover the new request/caching contract.
- [ ] A local `llmusage serve` or fixture-based browser check verifies that no visible layout overlap or error state is introduced.

## Out Of Scope

- Changing parser, sync import, pricing, or storage semantics.
- Adding remote telemetry, external services, or background network calls.
- Rewriting the dashboard as a client-side analytics engine.
- Redesigning the dashboard layout beyond loading states needed for responsiveness.
- Persisting long-lived dashboard caches across process restarts unless later evidence proves in-process caching is insufficient.

## Product Decision

- The user approved the stale-while-refresh direction for range switching:
  - first-screen KPI/trend content should update quickly;
  - Activity / Tools / Optimize / Explorer / Compare may temporarily show previous data with an explicit refreshing state;
  - the UI must not imply stale secondary data is already current for the newly selected range.

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
