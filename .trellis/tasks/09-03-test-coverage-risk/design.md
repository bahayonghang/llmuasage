# Design: risk-ranked test additions

## Boundaries

- Tests only. Production functions, error types, HTTP codes, and SQL stay unchanged.
- Prefer existing fixtures: `TempDir` + `Store::bootstrap()`, `tests/query/logs.rs` fixture, `src/web/mod.rs` `make_store` / `route_json`, `tests/sync/runtime/reset.rs` seed helpers.
- Put unit tests next to the code (`#[cfg(test)]` in the same module) unless an integration target already owns the surface (`tests/query/logs.rs`, `tests/sync/runtime/reset.rs`).

## Data flow under test

```
CLI/Web params → ValidatedSyncRequest / QueryFilter / ExplorerQuery
                → Dashboard/Store reads
Write path: permit → BEGIN IMMEDIATE → mutate → permit → COMMIT
Forget: ConnectInfo peer → reject_non_local_write → source parse → mark_source_file_deleted
Public read: public_dashboard_filter_from_params strips project_hash and host_id
```

## Module map

| Order | Module | Primary files | Test home |
| ---: | --- | --- | --- |
| 1 | Store tx / reset / cursor | `src/store/lock.rs`, `schema.rs`, `cursor.rs` | `src/store/lock.rs` tests; `tests/sync/runtime/reset.rs`; new tests in `cursor.rs` |
| 2 | Query filter / logs | `src/query/filter.rs`, `src/query/logs.rs` | existing `#[cfg(test)]` in filter.rs; `tests/query/logs.rs` |
| 3 | Web permission / params | `src/web/mod.rs`, `src/query/explorer.rs` | `src/web/mod.rs` tests; explorer sanitize in `explorer.rs` |
| 4 | Sync bounds | `src/sync/types.rs` | existing `types.rs` tests |
| 5 | Remote host_id | `src/remote/register.rs` | existing `register.rs` tests |
| 6 | Subscription HTTP | `src/subscription/http.rs` | new `#[cfg(test)]` in `http.rs` |

## Contracts to obey

- Write fencing: tests that mutate must use a bootstrapped Store (unfenced `write_transaction` acquires `HolderKind::Library` itself via `write_operation`).
- Web: reuse `route_json`; forget tests run against `WriteExposure::LocalOnly` loopback.
- Explorer HTTP 400 shape matches metric test: `error.code == "invalid_query"`.
- Logs cursor empty-field uses `decode` path already mapped to `LlmusageError::Io` InvalidInput; query-layer test can call `logs()` with a well-formed base64 JSON of empty strings and expect `Err`.
- `validate_new_host_id("codex")` uses a real registered source id, not a hardcoded extra list.

## Trade-offs

- No llvm-cov gate: CI has no coverage job; ranking by risk matches the request better than a percentage.
- Dashboard silent-degrade stays untested-as-desired and unfixed: locking the bug in would make a later 400 change fail those tests for the wrong reason.
- `write_transaction` rollback relies on rusqlite Drop; the test asserts the product-visible outcome (row count), not the SQLite rollback API.

## Rollback

Delete the added test functions / modules. No schema or API migration.
