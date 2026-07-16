# 优化看板时间范围切换性能

## Goal

Eliminate the multi-second pause observed when switching the web dashboard's
quick time-range preset (1d / 7d / 30d / all), while preserving correct
filtered data and a responsive interaction throughout refresh.

## User Value

Changing the dashboard time range is a first-screen comparison workflow. The
control must acknowledge the click immediately, update the visible range data
without waiting on off-screen analytics, and remain usable when the local
database grows beyond 500k events.

## Confirmed Facts

- The current local database is 1.11 GB and contains 523,644 `usage_event`
  rows, 234,147 `usage_turn` rows, 162,616 `usage_tool_call` rows, 6,859
  `usage_bucket_30m` rows, and 7,382 `source_cursor` rows.
- A live 3-run baseline against `127.0.0.1:37422` measured `scope=core` at
  639-839 ms and 1.71-1.76 MB. Full snapshots took 2.00-2.72 s for
  1d/7d/30d and 4.04-5.38 s for all.
- `health.cursors` contributes about 1.69 MB of the core payload. The browser
  only reads the array length; it does not render the 7,382 cursor records.
- A quick-range click waits for core, renders, then starts a full dashboard
  request. The full handler computes core again before loading secondary
  sections, so one click duplicates all core queries and serialization.
- `Dashboard::core_snapshot` computes day/week/month/all trend series even
  though the live view selects one trend window.
- The 7d section baseline identified Explorer at about 1.66 s, Optimize and
  Tools at about 1.01 s each, Compare at about 0.92 s, and Activity at about
  0.54 s. These sections are joined and published only after the slowest work
  completes.
- Default Explorer reads `usage_event` twice (rank rows plus time series), even
  though its default source/cost/day query can be answered by
  `usage_bucket_30m` (6,859 rows instead of 523,644 rows).
- `EXPLAIN QUERY PLAN` for the default event time-series query reports
  `SCAN e` plus temporary B-trees. A 7d source/day cost aggregate took
  805-1,005 ms. Time-only filters also cannot seek into the existing
  `(source, event_at)`, `(source, started_at)`, and `(source, occurred_at)`
  indexes when source is unfiltered.
- The server already runs SQLite work in `spawn_blocking` and uses
  `tokio::join!`, but timeout or browser abandonment does not cancel the
  blocking SQLite statement. Increasing async concurrency alone would add
  contention rather than remove work.
- The June 5 task `06-05-serve-dashboard-range-performance` introduced the
  current core/full stale-while-refresh path and validated it on a 240-row
  fixture. This task is a scalability follow-up, not a repeat of that design.

## Requirements

- Establish a repeatable measurement for the end-to-end preset-switch path,
  covering click response, network/query latency, main-thread work, and render
  completion.
- Identify the dominant bottleneck(s) across the browser refresh pipeline,
  `/api/dashboard`, dashboard query composition, and SQLite access.
- Keep range selection immediately responsive and prevent stale responses from
  overwriting the latest selection.
- Evaluate asynchronous execution, request cancellation/coalescing, caching,
  query/index optimization, payload reduction, and incremental rendering based
  on measured evidence rather than adopting all techniques unconditionally.
- Preserve the existing filter semantics, dashboard panels, degraded/fallback
  behavior, localization, and sync command-center behavior.
- Add regression coverage and a performance budget that can catch the reported
  multi-second interaction delay.
- Keep the existing full snapshot/export contract compatible while introducing
  a lean interactive payload or projection.
- Prefer the existing aggregate read model for compatible Explorer queries;
  use new indexes only for paths that must still query event/turn/tool facts.
- Do not increase the dashboard semaphore as a substitute for query reduction.

## Acceptance Criteria

- [x] A documented, agent-runnable benchmark reproduces the preset-switch path
      against representative data and records browser, HTTP, and query timings.
- [x] The selected preset and loading/stale state update within 100 ms of the
      click at browser p95, independent of network/query completion.
- [x] On the current representative database, the cold interactive range API
      completes within 400 ms p95 after one warm-up and returns at most 128 KiB.
- [x] The range interaction does not issue a subsequent request that recomputes
      the same core snapshot; secondary sections refresh independently.
- [x] Rapid range changes render only the newest requested range; obsolete work
      is aborted/ignored, active work stays bounded, and abandoned SQLite work
      is interrupted when it has already started.
- [x] Default Explorer on 1d/7d/30d/all uses the aggregate read model when its
      filters/dimensions permit and remains result-equivalent to event rows.
- [x] Secondary panels progressively reach the selected range without blocking
      the first-screen update; no panel labels stale data as current.
- [x] Dashboard values remain equivalent to the current implementation for the
      same filter, including fallback/degraded responses.
- [x] Targeted frontend and Rust tests cover request lifecycle, stale-response
      handling, query results, and the chosen performance-sensitive boundary.
- [x] `just ci` passes before completion.

## Out of Scope

- Visual redesign of dashboard panels or filter controls.
- Changes to usage ingestion/sync semantics unrelated to read performance.
- A database schema migration unless query-plan evidence proves it necessary.

## Product Decision

- The user approved a forward-only schema migration adding standalone time
  indexes after aggregate-read-model routing lands.
- Approval is conditional rather than a mandate: keep an index only when the
  remaining event/turn/tool query plan changes from a full scan to an indexed
  search, representative reads improve materially, migration time remains
  acceptable, and measured sync write amplification is justified.
- The user accepted the performance budgets: browser click feedback p95 at or
  below 100 ms, interactive API p95 at or below 400 ms after warm-up, and an
  interactive response no larger than 128 KiB on the representative database.
- The user approved starting implementation after reviewing the completed PRD,
  design, and implementation plan.

## Notes

- User-reported symptom on 2026-07-11: changing the highlighted quick time
  range can stall the page for several seconds.
- Exact browser click-to-render timing could not be captured during planning
  because the in-app browser surface had no available tab. HTTP, payload, SQL,
  source, and history evidence above were captured live; the implementation
  begins by adding the missing browser-runnable timing harness.
- Existing `Cargo.toml` and `Cargo.lock` modifications predate this task and
  must not be overwritten or folded into the performance work accidentally.
