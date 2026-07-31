# Fix serve behavior analytics query timeout

## Goal

Restore usable Behavior analytics in `llmusage serve` on the current local usage database. Activity, Tools, Optimize, and Compare must return their real supported/no-data result instead of degrading because the dashboard query exceeded the fixed 1000 ms deadline.

## Background

- The supplied browser screenshot shows Activity succeeding while Tools, Optimize, and Compare render `invalid config: dashboard query exceeded 1000 ms timeout`.
- The loopback dashboard intentionally loads Behavior sections independently and preserves per-section degraded states.
- `src/web/mod.rs` currently applies `WEB_BEHAVIOR_API_TIMEOUT = 1s` to each Behavior section.
- A clean-server HTTP reproduction shows all four unbounded endpoints timing out with zero semaphore wait. The current database contains 176,898 events, 172,676 turns, and 157,903 tool calls.
- Direct read-only profiling measures unbounded Activity at about 3.0 seconds and Tools at about 10.8 seconds. Tools builds an automatic `event_key` index and several temporary B-trees. Optimize spends about 1.6 seconds in low-read/edit and 2.3 seconds in session-outlier because both join every fact row to `usage_event` before reducing it.
- The live schema is version 17 but lacks the expression index now declared inside the historical v11 migration. Editing an already-applied migration did not repair existing databases; any index repair must be a new migration.
- Indexed-copy experiments show indexes alone cannot satisfy the all-history path. Equivalent Optimize prototypes reduce the two dominant detectors to roughly 0.4 seconds each, and flat event/turn/tool projections show that sequential reads are materially cheaper than the current random-lookup/temporary-aggregation plans.

## Requirements

- R1: Reproduce the exact per-section 1000 ms timeout through the loopback HTTP API against the current local database, with a deterministic command or focused automated test.
- R2: Identify the measured bottleneck and fix the responsible query, index, scheduling, or deadline policy at the narrowest correct layer.
- R3: Preserve independent section responses and explicit `normalized`, `no_data`, `unsupported`, and genuine `degraded` states.
- R4: Preserve loopback/public route boundaries, full/static snapshot compatibility, range/window filters, cancellation, and bounded SQLite busy handling.
- R5: Add a regression test at the real query/web seam that fails for the reproduced timeout pattern before the fix and passes afterward.
- R6: Keep the change local to Behavior query performance and orchestration unless measurements prove a shared dashboard primitive is the cause.
- R7: Add a forward schema migration for the final query-plan indexes; it must repair an already-versioned v17 database as well as fresh databases.
- R8: Keep default `1d` Behavior reads targeted below one second on the representative database. All-history reads may use a bounded three-second Behavior deadline after the query work is complete; the general five-second API deadline remains unchanged.
- R9: Do not treat simply increasing or removing the one-second timeout as sufficient. The timeout change is accepted only together with measured query-plan improvements and must preserve cancellation/degraded behavior for genuine overruns.
- R10: Because `llmusage serve` runs bootstrap and v18 prevents older binaries from opening the database, create and verify a separate pre-v18 SQLite backup before the first live-current-database validation.

## Acceptance Criteria

- [ ] The original HTTP reproduction no longer returns `dashboard query exceeded 1000 ms timeout` for Activity, Tools, Optimize, or Compare on the current local database.
- [ ] A focused deterministic regression test covers the root-cause pattern and passes.
- [ ] A v17-shaped database missing the required indexes upgrades through the new migration and exposes every final index; fresh bootstrap exposes the same schema.
- [ ] A verified pre-v18 backup exists before the current local database is migrated for live validation, and its restore path is recorded in task evidence.
- [ ] Query equivalence tests prove Activity, Tools, Optimize, and Compare payload semantics across empty, filtered, multi-tool, non-tool, orphan, and low-sample cases.
- [ ] Each Behavior section still independently represents normalized, no-data, unsupported, and real error/degraded outcomes.
- [ ] Existing dashboard API scope/route/cancellation contract tests pass.
- [ ] The relevant Rust test slice and the repository `just ci` gate pass.
- [ ] Three-sample representative measurements document `1d` and `all` before/after wall time for each section plus a concurrency-2 browser loading round; no sample may degrade due to the Behavior deadline.

## Out of Scope

- Changing parser normalization, token/cost accounting, or stored user usage facts.
- Redesigning the Behavior UI or its copy.
- Reworking unrelated dashboard sections such as Explorer, Health, or Home Overview.
- Removing deadlines, silently serving stale data, or hiding genuine query errors.
- Changing the deliberate PERF-002 hard-timeout behavior where interrupted blocking work owns its permit until background cleanup completes.
