# Implement

## Checklist

- [x] Review this plan with the user and resolve the stale-while-refresh versus atomic-update decision.
- [x] Start the Trellis task after approval.
- [x] Load `trellis-before-dev` before editing.
- [x] Add frontend request-key normalization and in-flight coalescing.
- [x] Prevent duplicate default Explorer loading when `/api/dashboard` already returned Explorer.
- [x] Add a scoped first-screen dashboard fetch path or equivalent fast refresh path.
- [x] Keep heavy secondary sections stale-while-refresh according to the approved product decision.
- [x] Add cache invalidation on sync job completion/manual refresh and filter changes.
- [x] Add focused tests for fetch orchestration and API scope behavior.
- [x] Measure local endpoint and browser interaction latency before/after.
- [x] Run targeted tests, then broader repo gates as practical.

## Suggested Implementation Order

1. Frontend instrumentation and tests.
   - Verify current range click calls full `reloadDashboard`.
   - Add tests for the desired fetch contract before changing behavior.

2. Remove duplicate Explorer load.
   - Reuse `snapshot.explorer` from `/api/dashboard`.
   - Load `/api/explorer` only for Explorer-specific changes or missing payloads.

3. Add request cache and coalescing.
   - Implement a bounded TTL cache for JSON requests.
   - Coalesce identical in-flight keys.
   - Add explicit invalidation hooks.

4. Add fast range refresh.
   - Either introduce a scoped `/api/dashboard` response or a first-screen fetch path.
   - Keep existing full snapshot behavior for export/backwards compatibility.

5. Defer heavy panels.
   - Render stale secondary panels with a refreshing marker or leave them unchanged until refreshed.
   - Preserve degraded state semantics.

6. Measure and tune.
   - Re-run local endpoint timings.
   - Inspect payload size and request count in browser automation.
   - Only then consider backend query/index/cache work if still necessary.

## Validation Commands

```powershell
rtk cargo test web::tests::
rtk cargo test query::
rtk cargo fmt --check
rtk git diff --check
rtk just ci
```

Use targeted names after implementation adds specific tests.

## Performance Checks

Use a running live server or docs fixture with representative data:

```powershell
rtk powershell -NoProfile -Command "$urls = @('/api/dashboard?window=day','/api/dashboard?window=week','/api/dashboard?window=month','/api/dashboard?window=all','/api/explorer?range=all&granularity=day&metric=attributed_cost_usd&group_by=source&limit=8&include_other=true'); foreach ($u in $urls) { $sw=[Diagnostics.Stopwatch]::StartNew(); $r=Invoke-WebRequest -UseBasicParsing -Uri \"http://127.0.0.1:37422$u\" -TimeoutSec 30; $sw.Stop(); \"$u status=$($r.StatusCode) bytes=$($r.RawContentLength) ms=$($sw.ElapsedMilliseconds)\" }"
```

Also verify browser-side request count and time-to-updated-trend after clicking each range button.

Verified on the docs fixture server at `127.0.0.1:37431` with 240 seeded rows:

- Initial live load requested one full `/api/dashboard?window=day&range=1d` payload and did not issue a duplicate `/api/explorer`.
- Range clicks requested `/api/dashboard?...&scope=core` first, then a background full `/api/dashboard` when not satisfied by the 10s live cache.
- Desktop click frame times were 5-17 ms on the fixture, core API responses were 6-7 ms, and no console errors were observed.
- Mobile viewport `390x844` had no horizontal overflow.

## Risky Files

- `src/web/assets/app.js`: central reload/render orchestration.
- `src/web/assets/data/fetch.js`: request caching and endpoint selection.
- `src/web/mod.rs`: API route/scope behavior and degraded response contract.
- `src/query/mod.rs`: core snapshot composition.
- `src/query/explorer.rs`: expensive Explorer aggregation.

## Rollback Point

If scoped/stale-while-refresh behavior gets too complex, keep only duplicate Explorer removal and in-flight request coalescing, then re-measure before adding backend cache.
