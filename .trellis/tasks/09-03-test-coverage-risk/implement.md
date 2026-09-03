# Implement: risk-ranked tests

Do not change production logic. After each module, run that module's command before starting the next.

## 1. Store transaction / reset / cursor

- Add `write_transaction` rollback test in `src/store/lock.rs`: insert one meta/event row, return `Err`, assert count 0.
- Extend `tests/sync/runtime/reset.rs`: seed `run_log` + `integration_install`, call `reset_usage_data`, assert those rows remain and usage/behavior tables are empty.
- Add `src/store/cursor.rs` tests: persist a `source_cursor` row with `last_total_json` that is not JSON; `load_file_cursors` returns `last_total = None`.

```
cargo test --lib store::lock -- --test-threads=1
cargo test --test sync reset_usage_data -- --test-threads=1
cargo test --lib store::cursor -- --test-threads=1
```

## 2. Query filter / logs

- `src/query/filter.rs`: whitespace-only host/model/project produce no column predicates; `until` at `NaiveDate::MAX` omits the until clause; `since`-only still emits `>=`.
- `tests/query/logs.rs`: three events; `page_size=0` length ≤ 50; `page_size=1000` length ≤ 500; `page_size=1` yields `next_cursor` and the second page is the remaining row; cursor JSON with empty `event_key` fails.

```
cargo test --lib query::filter -- --test-threads=1
cargo test --test query logs -- --test-threads=1
```

## 3. Web permission / explorer params

- `public_dashboard_filter_from_params` with `host` and `host_id` keys → `filter.host_id == None`.
- Loopback POST `/api/diagnostics/forget` without `source` → 400 `missing_source`; `source=not-a-source` → 400 `unknown_source`.
- GET `/api/explorer?granularity=bogus`, `group_by=bogus`, `token_type=bogus` → 400 `invalid_query`.
- `sanitize_query` clamps `limit` 0 and 999 into `1..=50`.

```
cargo test --lib web public_dashboard_filter -- --test-threads=1
cargo test --lib web forget -- --test-threads=1
cargo test --lib web explorer_api_rejects -- --test-threads=1
cargo test --lib query::explorer sanitize -- --test-threads=1
```

## 4. Sync request bounds

- Extend `src/sync/types.rs` table: `parallelism=0` → `InvalidParallelism`; `recent_days=3651` → `InvalidRecentDays`; `recent_days=3650` succeeds.

```
cargo test --lib sync::types -- --test-threads=1
```

## 5. Remote host_id

- Empty `host_id`; `codex` collision; second insert of the same id after a successful `upsert`. All `ConfigInvalid`.

```
cargo test --lib remote::register -- --test-threads=1
```

## 6. Subscription HTTP

- `status_error("Codex", UNAUTHORIZED)` contains `stored access token was rejected`.
- `status_error("Codex", INTERNAL_SERVER_ERROR)` contains `usage request failed` and not the token phrase.

```
cargo test --lib subscription::http -- --test-threads=1
```

## 7. Full suite

```
cargo test --all-features -- --test-threads=1
```

Optional format check on touched files:

```
cargo fmt --check
```

## Risky files

- `src/web/mod.rs` test module is large; add focused tests next to the existing public-filter and explorer-metric cases.
- `tests/sync/runtime/reset.rs` shares helpers with other reset tests; do not change `reset_for_source` behavior.

## Stop conditions

- A new test fails because production behavior differs from `prd.md`: stop, update planning, do not patch production in this task.
- Full suite failure in an untouched test: investigate before adding more cases.
