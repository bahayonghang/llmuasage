# Design: restore Behavior analytics under bounded deadlines

## Boundary

The change stays inside the SQLite read/query path, schema indexes, and the Behavior endpoint deadline. Parser output, stored fact meaning, cost accounting, dashboard payload JSON, frontend rendering, public route exposure, and PERF-002 cancellation ownership remain unchanged.

## 1. Forward index migration

Add schema migration v18. It creates only indexes consumed by the final plans:

- `usage_event(event_at)` for all-source bounded event projections;
- `usage_turn(started_at)` for all-source bounded turn projections;
- `usage_turn(session_id)` for the narrowed session-cost lookup;
- `usage_tool_call(event_key)` for event/tool attribution and non-tool detection;
- `usage_tool_call(occurred_at)` for all-source bounded tool projections;
- `usage_tool_call(model, occurred_at)` for selected-model tool counts;
- `idx_usage_turn_event_key_expr` again, so already-versioned databases receive the index currently stranded in historical v11.

Migration tests start from schema v17 with the indexes absent, run bootstrap, and assert v18 plus every expected plan index. Fresh bootstrap must produce the same set. Index creation is transactional; existing usage facts are not rewritten.

## 2. Activity sequential aggregation

Replace one random `usage_event` lookup per turn with two ordered projections on the same read connection:

1. load the exact `event_key -> persisted cost` projection;
2. stream filtered `usage_turn` facts and aggregate category counts/tokens/rates in Rust, looking up cost in memory.

Turns without a matching event retain zero attributed cost. Sorting remains cost, tokens, turns, then category. The internal map is request-local and released with the payload; no cross-request stale cache is introduced.

## 3. Tools sequential attribution

Replace the multi-CTE SQL materialization with typed projections and a Rust reducer:

1. load event attribution fields keyed by event key;
2. stream filtered tool facts, count siblings per event, and retain the exact turn/session identities needed for distinct counts;
3. attribute event cost/tokens equally across filtered siblings;
4. emit filtered events with no filtered tool fact into the existing `(non-tool)` bucket;
5. sort and truncate to the same top 50 rows.

The implementation must preserve the current asymmetry: tool rows are governed by `tool_filter` and borrow their linked event's persisted values, while non-tool rows are governed by `event_filter`. Orphan tool rows remain excluded. Test-only legacy SQL stays as an equivalence oracle until coverage passes.

## 4. Optimize and Compare query consolidation

Optimize:

- `behavior_support` uses `EXISTS` rather than counting every matching row;
- low-read/edit counts calls without an event join and joins only edit rows for cost/tokens;
- duplicate/junk detectors retain their existing filters and thresholds;
- session outlier chooses the top session and total turn tokens before joining costs for only that session, eliminating the second full joined scan.

Compare:

- preserve the existing single candidate query;
- aggregate both selected models together for bucket stats, turn stats, tool counts, and category head-to-head instead of issuing the same query family once per model;
- keep missing-model, low-sample, metric, category, and working-style output byte-for-byte equivalent after JSON serialization.

## 5. Deadline and concurrency

After the query changes, set the Behavior-specific deadline to three seconds. The general API deadline stays five seconds. Browser secondary concurrency remains two, and each section still settles independently.

The three-second ceiling is not the performance target: representative `1d` median should remain below one second, while exact `all` reads must complete below three seconds without degradation. Real overruns, lock errors, and cancellations still produce the existing section-local degraded payload.

## Compatibility and rollback

- No API fields or support levels change.
- Static/full exports continue using complete exact payloads.
- `llmusage serve` calls `Store::bootstrap()` before binding, so its first v18-aware live run performs the migration. Before that run, create a separate SQLite online backup, verify it opens as schema v17, and retain its exact path in task evidence.
- v18 is index-only, but older binaries will reject the newer schema version. Operational rollback after migration requires restoring that verified pre-migration database backup or continuing with a v18-aware binary; dropping indexes alone is not a supported version rollback.
- If exact Tools aggregation cannot meet equivalence or the three-second representative ceiling, stop rather than approximate distinct counts/costs. The fallback is a separately planned persistent Behavior rollup, not silent semantic weakening.
