# Behavior query timeout diagnosis

## Reproduction

The red-capable loop starts the current debug binary against the normal local runtime and requests `/api/activity`, `/api/tools`, `/api/optimize`, and `/api/compare` concurrently. It fails when any `support.reason` contains `dashboard query exceeded 1000 ms timeout`.

Observed clean-server result:

| Section | Wall time | Result |
| --- | ---: | --- |
| Activity | 1.09 s | degraded, exact 1000 ms timeout reason |
| Tools | 1.01 s | degraded, exact 1000 ms timeout reason |
| Optimize | 1.02 s | degraded, exact 1000 ms timeout reason |
| Compare | 1.01 s | degraded, exact 1000 ms timeout reason |

Debug tracing recorded `semaphore_wait_ms=0` and `query_ms` around 1003-1011 ms for each isolated request. A fresh process therefore reproduces the bug without permit contention or leftover detached work.

## Current scale and plans

The profiler in `research/profile_behavior_queries.py` opens SQLite with `mode=ro`, emits only counts/timings/query-plan operators, and never prints model, path, session, or fact values.

| Table | Rows |
| --- | ---: |
| `usage_event` | 176,898 |
| `usage_bucket_30m` | 4,692 |
| `usage_turn` | 172,676 |
| `usage_tool_call` | 157,903 |

Unbounded direct SQL:

| Query | Before | Dominant plan evidence |
| --- | ---: | --- |
| Activity | 2.996 s | turn scan plus one PK event lookup per turn |
| Tools | 10.764 s | tool/event scans, automatic `event_key` index, group/distinct/order temp B-trees |
| Optimize low-read/edit | 1.626 s | full tool scan plus event PK lookup per tool |
| Optimize duplicate reads | 0.223 s | kind index plus group/order temp B-trees |
| Optimize junk reads | 0.105 s | kind index plus selected event lookups |
| Optimize session outlier | 2.284 s | full turn/event join before group/order |

For the live `1d` range, Activity falls below the deadline, but Tools remains around 0.94 seconds in direct SQLite before connection, support, Rust, and HTTP overhead. Optimize's component queries plus support and total-token queries also leave insufficient margin.

## Index experiment

A SQLite online backup was placed under ignored `target/tmp/behavior-query-profile/`. Candidate time/event indexes and the missing historical expression index were created only on that copy.

Indexes made bounded plans use range searches, but were not sufficient alone: unbounded Tools remained 6.6-8.5 seconds and Activity 1.8-2.8 seconds. The final fix must change the reduction shape, not merely append indexes.

The live v17 database lacks `idx_usage_turn_event_key_expr`, even though current source declares it in migration v11. Existing databases already past v11 never rerun that edited migration. This must be repaired in a new versioned migration.

## Rewrite experiments

- Low-read/edit: count calls without joining events, then join only edit calls for cost. On the indexed copy this changed about 1.98 s to 0.41 s unbounded.
- Session outlier: choose the top session from turn facts first, then compute cost only for that session. This changed about 2.18 s to 0.39 s unbounded.
- Flat projections on the live database read all event costs plus all turn facts in about 0.64 s total, compared with about 3.0 s for Activity's per-turn event lookups.
- Flat projections read all event attribution fields plus event-backed tool calls in about 0.80 s total before aggregation, compared with about 10.8 s for Tools' SQL materialization and temporary distinct/group trees.

These probes support typed Rust aggregation over sequential SQLite projections for Activity/Tools, plus narrower SQL aggregation for Optimize/Compare. Output equivalence must be locked against the existing SQL behavior before removal of the old implementation.

## Hypothesis disposition

1. Full scans/random event lookups/temp aggregation are primary: confirmed.
2. Missing range/join indexes amplify bounded reads: confirmed, but index-only fix disproved.
3. Detached timeout cleanup is the root cause: disproved by clean-server isolated requests with zero permit wait. The existing PERF-002 lifecycle remains out of scope.
4. Deadline-only defect: disproved as a complete fix. A bounded deadline adjustment is still justified after query optimization because all-history exact aggregation cannot reliably fit the old one-second budget.
