# Design: query SQL performance

## Approach

Keep `Dashboard` as the façade. Replace Rust-side full-table folds with SQL that already exists in test oracles, then delete the production full scans.

| Path | Target SQL |
| --- | --- |
| activity | Promote `legacy_activity_breakdown` join (turn → event cost) and apply `QueryFilter`. |
| home overview | `GROUP BY source` / `COUNT(DISTINCT session identity)` / daily series from events or buckets. Prefer buckets for tokens/cost when filter allows; sessions still need event identity. |
| tools | Join `usage_tool_call` to filtered events in SQL; drop unused token columns on `AttributedToolRow`. |
| top_sessions | Production `load` uses the grouped query in `load_legacy` for tokens/cost; duration sort: grouped min/max(event_at) in SQL. |
| session `--id` | Add session predicate to `push_event_filter` / `QueryFilter`. |
| last_event_at | `SELECT source, MAX(event_at) FROM usage_event WHERE ... GROUP BY source`. |
| TUI context_pressure | Single `Dashboard::context_pressure(&filter)` without per-source loop. |
| overview | One statement returning the sums/counts now fetched separately, or a shared prepared filter CTE. |

`HOME_PLATFORMS`: build the card list from `registered_source_descriptors()` (or descriptors marked `home_card`). Do not drop registered parser sources.

## Compatibility

JSON field names stay. `support` degradation levels stay. Interactive snapshot section set stays.

## Rollback

Revert the query modules; tests in `src/query/tests` and `tests/query` fail closed if scans return.
