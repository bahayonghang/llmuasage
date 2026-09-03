# Coverage gap inventory (2026-09-03)

Static inventory of `#[test]` plus contract files. No `cargo llvm-cov` run in this pass.

## Inventory

| Surface | Count |
| --- | ---: |
| `src/**/*.rs` unit tests | 729 |
| `tests/**/*.rs` integration tests | 204 |
| Total | 933 |
| Dashboard JS (`scripts/tests/*.test.mjs`) | 7 files |

`Cargo.toml` sets `autotests = false`. Integration targets: `api`, `architecture_dependencies`, `cli`, `query`, `remote`, `store`, `sync`, `tui`.

## Already covered (do not retest for coverage)

High-density areas that already exercise the contracts this task cares about:

- Write fencing steal / expired refresh / `commit_shard` LockLost: `src/store/lock.rs:563-703`, `src/store/sync_writer.rs:1512-1552`
- Schema too new / corrupt version / migration rollback: `src/store/migrations.rs:1443+`, `src/store/schema.rs:412-441`
- Sync job request validation (unknown source, `recent_days=0`, parallelism cap): `src/sync/types.rs:196-223`, `src/sync/job_registry.rs:1061-1099`, `src/web/mod.rs:6537-6595`, `tests/api/facade.rs:40-54`
- Public vs loopback route allowlist and mutation absence: `src/web/mod.rs:2313-2360`
- Explorer invalid metric HTTP 400: `src/web/mod.rs:5441-5453`
- Logs malformed cursor HTTP 400: `src/web/mod.rs:4824-4831`
- Token accounting, parsers, pricing, TUI panels, source sync regressions: existing `tests/sync/**` and `src/query/**` suites

## High-risk gaps (in scope)

Ranked by business risk. Each item is a missing test for **existing** behavior.

### 1. Store transaction rollback

- `Store::write_transaction` (`src/store/lock.rs:186-199`) validates permit, runs the closure, then commits. If the closure returns `Err`, the `Transaction` is dropped without `commit` and rusqlite rolls back. No test inserts a row then fails the closure and asserts the row is absent.
- `reset_usage_data` (`src/store/schema.rs:264-295`) now wraps deletes in `write_transaction`. `tests/sync/runtime/reset.rs:72-87` only asserts usage/behavior tables are empty. It does not assert `run_log` / `integration_install` survive (the comment at `schema.rs:273` says they must).
- `src/store/cursor.rs` has no `#[cfg(test)]`. `last_total_json` / processed-id JSON use `serde_json::from_str(...).ok()` (`cursor.rs:60-62`, `95-97`, `172-174`). Corrupt JSON currently loads as empty/default rather than failing the read.

### 2. Query filter and logs bounds

- `QueryFilter` skips whitespace-only `host_id` / `model` / `project_hash` (`src/query/filter.rs:115-146`). Tests cover timezone bounds and non-empty `host_id` (`filter.rs:247-336`), not the empty/whitespace skip.
- `until.succ_opt()` (`filter.rs:155-163`): if `until` has no successor, the until clause is omitted. No test.
- `LogsQuery` page size clamp (`src/query/logs.rs:351-356`): `0 → 50`, `>500 → 500`. `tests/query/logs.rs` has one test for session/detail mode; no pagination `next_cursor`, no page-size clamp, no empty cursor fields (`logs.rs:338-340`).

### 3. Web permission and remaining parameter validation

- Public filter zeros `project_hash` **and** `host_id` (`src/web/mod.rs:1852-1856`). Test `public_dashboard_filter_ignores_project_selectors` (`web/mod.rs:2326-2339`) only checks project keys.
- `POST /api/diagnostics/forget` returns `missing_source` / `unknown_source` (`web/mod.rs:1216-1238`) and uses `reject_non_local_write`. Public router absence is tested; the 400 codes are not.
- Explorer HTTP rejects unsupported metric. `explorer_query_from_params` also rejects unsupported `granularity` / `group_by` / `token_type` (`web/mod.rs:1878-1900`). Those 400 paths are untested.
- Query-layer `sanitize_query` clamps explorer `limit` to `1..=50` (`src/query/explorer.rs:334-337`). Untested.

### 4. Sync request remaining bounds

- `ValidatedSyncRequest::new` rejects `parallelism` outside `1..=32` and `recent_days` outside `1..=3650` (`src/sync/types.rs:87-107`). Unit cases cover unknown source, `recent_days=0`, `parallelism = MAX+1`. Missing: `parallelism=0`, `recent_days=3651`, accepted `recent_days=3650`.

### 5. Remote host_id validation

- `validate_new_host_id` rejects empty id, collision with a source name, and duplicate host (`src/remote/register.rs:26-51`). Tests cover `normalize_host_id` and handshake/binary failures, not these three `ConfigInvalid` branches.

### 6. Subscription HTTP status mapping

- `subscription::http::status_error` (`src/subscription/http.rs:13-22`) special-cases 401/403. Callers: claude/codex/grok/kimi. Only `subscription/cache.rs` has tests (TTL). 401 vs other status copy is untested.

## Deferred (out of scope for this task)

- Dashboard query `source` / `window` / `since` still silently degrade (`dashboard_filter_from_params_without_window` at `web/mod.rs:1859-1871`, `apply_window_filter` `_ => {}` at `1996-2020`, `query_timezone` unknown → `Local` at `1944-1963`). Jobs/sync already 400 on unknown source. Changing query semantics is a product change, not a test-only task.
- TUI presentation, theme, format helpers, `commands/{daily,weekly,monthly}.rs` thin wrappers (covered by `tests/cli/reports.rs`).
- Dashboard JS rendering tests unless they encode API contracts already listed.
- `cargo llvm-cov` percentage target. `docs/reports/llmusage_optimization_canvas.md` mentioned 80% as a future idea; it is not a current CI gate.
- Live-home database, network quota fetchers, SSH remote.

## Prior tasks that already closed related holes

- `07-24-api-input-validation` — sync/job typed validation (landed for jobs/CLI; dashboard query silent-degrade remains).
- `07-24-store-robustness` — SchemaTooNew / SchemaVersionCorrupt / reset transaction wrap.
- `07-26-public-read-security-boundary` — public allowlist and loopback-only reads.
- Write-fencing contracts — steal tests and stale `commit_shard` exist.
