# Design: store/query decouple

## Approach

Move the types store already needs out of `query`:

- `src/query/pricing.rs` → `src/domain/pricing.rs` (or `src/store/cost.rs` if you want store-owned). Prefer `domain` so both store and query depend downward.
- SQLite function registration (`query/timezone.rs` `register_functions`, `FN_LOCAL_*`) → `src/store/sqlite_functions.rs` or `src/domain/sqlite_time.rs`. Query keeps `ReportTimezone` and SQL expression builders that *call* those function names.

`query/pricing_catalog.rs` can stay in query if store only needs `CostBreakdown` + `compute_cost_with`. Store's `pricing_catalog.rs` already owns catalog files; it should import domain cost types, not `query::`.

## Architecture test

Copy the existing `is_commands_dependency` visitor. Add `store_does_not_depend_on_query` walking `src/store`. Fixture under `tests/architecture/fixtures/` that `use crate::query::...` inside a fake store file, asserting detection. Production scan uses real `src/store`.

## Compatibility

Re-export `llmusage::query::{CostBreakdown, PricingStatus, ...}` from the new module so embedders do not break.

## Tradeoff

Do not merge `ReportFilter` in this task.
