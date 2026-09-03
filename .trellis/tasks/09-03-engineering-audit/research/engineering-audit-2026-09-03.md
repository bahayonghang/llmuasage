# Engineering audit 2026-09-03

Tier: deep. Scope: first-party `src/`, `tests/`, `scripts/`. Excluded: `target/`, `ref/`, `docs/node_modules/`. Product code was not modified.

Prior 2026-07 audit closure (`07-24-*`, `07-26-*`) still holds for public route allowlist, write fencing, bounded JSONL, JobRegistry validation, and loopback vs `--public` routers. This pass is a new baseline, not a delta of that report.

## Commands and evidence

| Check | Result |
| --- | --- |
| `git log --since=2026-07-01 --name-only src` | Hotspots: `web/mod.rs` (42), `query/mod.rs` (30), `commands/sync.rs` (24), `store/mod.rs` (17) |
| File size | `web/mod.rs` 6241 lines (tests from ~2082); `query/reports.rs` 4138; `store/migrations.rs` 2797 |
| TODO/FIXME in `src/` | none |
| Architecture tests | forbid `query`→`commands/web/tui` and `sync/remote`→`commands`; do not forbid `store`→`query` |
| `cargo test` / clippy / audit advisory DB | missing evidence (not run; network-sensitive tools not used) |

## Finding inventory

IDs below are the source of truth for child mapping. Severity uses the auditor contract. Effort: S ≤ 1 day, M 2–4 days, L > 4 days.

| ID | Dimension | File:Line | Severity | Effort | Description |
| --- | --- | --- | --- | --- | --- |
| CORR-001 | Correctness | `src/parsers/antigravity.rs:345-378` plus `:309-325` | high | S | Open/prepare failure returns empty success; existing cursor still reset + advanced. Busy/corrupt conversation DB deletes stored events and skips retry. |
| PERF-001 | Performance | `src/query/activity.rs:75-85` | high | S | `activity_breakdown` loads every `usage_event` cost with no filter. Filtered SQL exists only as `#[cfg(test)]` `legacy_activity_breakdown`. |
| PERF-002 | Performance | `src/query/home_overview.rs:336-372` | high | M | Home overview / compact load every matching event into RAM. Period reports already use `usage_bucket_30m`. |
| PERF-003 | Performance | `src/query/tools.rs:190-266` | high | M | Tool attribution materializes all tool rows and filtered events, then splits cost in Rust. |
| PERF-004 | Performance | `src/query/top_sessions.rs:223-293` | high | M | Production path projects every matching event; grouped SQL exists only as test-only `load_legacy`. |
| PERF-005 | Performance | `src/query/reports.rs:1238-1289` | high | M | Session report / `--id` walks all filtered events in Rust; no `session_id` SQL predicate. |
| ARCH-001 | Architecture | `src/store/mod.rs:14-18`, `src/store/connection.rs:57-58` | high | M | Store imports `query::pricing` and registers `query::timezone` functions. Architecture tests never scan `store`→`query`. |
| ARCH-002 | Architecture | `src/query/reports.rs:26-36`, `src/query/breakdowns.rs:261-279` | high | L | Two read façades (`Dashboard` + `ReportFilter`). `blocks_report` opens a second connection. Filter SQL can drift. |
| SEC-001 | Security | `src/web/mod.rs:1208-1381` | medium | M | Loopback writes (`POST /api/jobs`, forget-file) check TCP peer only. No CSRF token, Origin, or Host allowlist. |
| SEC-002 | Security | `src/remote/transport.rs:21-30` | medium | S | `ssh_target` is a single argv with no `--`. A target starting with `-` is another OpenSSH option. |
| SEC-003 | Security | `src/commands/codex_tracer/server.rs:38-41`, `:241-310` | medium | S | GET `/api/refresh` mutates tracer DB. Error bodies include `err.to_string()` and paths. |
| CORR-002 | Correctness | `src/parsers/opencode.rs:121` | medium | S | OpenCode opens the user DB read-write with default busy timeout 0. Antigravity/ZCode use `SQLITE_OPEN_READ_ONLY`. |
| CORR-003 | Correctness | `src/store/schema.rs:277-290` vs `reset_for_source_tx` | medium | S | Global `reset_usage_data` does not delete `source_file`. Forgotten files survive a full wipe. |
| CORR-004 | Correctness | `src/sync/engine.rs:513-518` | medium | S | Rebuild reset is one transaction per source. A later source failure leaves a mixed database. |
| CORR-005 | Correctness | `src/store/pricing_catalog.rs:437-445` | medium | M | Crash recovery loads `active_pricing_catalog()` (old meta) instead of the in-progress overlay target. |
| CORR-006 | Correctness | OpenCode/ZCode page commit vs later cursor write | medium | M | Cursor is a second `write_transaction` after `commit_shard`. Crash between them is not atomic. |
| PERF-006 | Performance | `src/query/breakdowns.rs:310-370` | medium | S | One `SELECT MAX(event_at)` per source/host after bucket aggregation. |
| PERF-007 | Performance | `src/tui/data_loader.rs:307-323` | medium | S | Stats panel spawns one `context_pressure` query per registered source when filter.source is unset. |
| PERF-008 | Performance | `src/query/overview.rs:310-347` | medium | S | Overview repeats the same bucket filter across ~8 sequential queries. |
| OBS-001 | Correctness | `src/commands/mod.rs:243-290` | medium | S | `run_tracked` wraps init/sync/serve/export/uninstall only. Default `daily` and catalog/logs/update have no `error!` / `run_log`. |
| SEC-004 | Security | `src/subscription/http.rs:6-10` | low | S | Default reqwest redirects can carry `Authorization` to another host. Endpoints are hardcoded HTTPS. |
| SEC-005 | Security | `src/query/explorer.rs:1123-1141` | low | S | `tool_name` / `tool_kind` interpolated with quote doubling instead of bound parameters. |
| SEC-006 | Security | `src/export/mod.rs:21-37`, `src/query/snapshot.rs:5-57` | medium | S | HTML export `snapshot.json` includes projects, hosts, `archive_root`, and failure text. Safety page says aggregates/labels only. |
| SEC-007 | Security | web asset responses; tracer HTML | low | S | No CSP, `X-Frame-Options`, or `X-Content-Type-Options`. |
| ARCH-003 | Architecture | `src/commands/codex_tracer/` | medium | L | Second SQLite + HTTP + parser island. Document as sidecar or ingest through Store. Merge is L; documenting is S and belongs in hygiene. |
| ARCH-004 | Architecture | `docs/adr/0003-store-facade-vs-substores.md` | low | S | ADR still lists `TriggerStore`. Code has HostStore + SourceFileStore and no trigger write API. |
| READ-001 | Readability | `src/commands/tui.rs`, `static_v1()`, `delete_for_source_in_tx` | low | S | Unused wrappers / deprecated API / dead helper. |
| OBS-002 | Architecture | `docs/architecture/index.md:103-104` vs `src/subscription/` | low | S | Architecture/safety say no remote usage API. TUI Usage tab fetches vendor quota with local tokens (spec `tui-subscription-contracts.md`). |

## Looks bad but is actually fine

- `format!` SQL table names in explorer/migrations use internal constants (`usage_turn`, `usage_tool_call`), not request strings. `QueryFilter` binds source/model/host/dates.
- `page.last().unwrap()` in `store/mod.rs:604` is after an empty-page break.
- Pi/Omp share `PiFormatParser` (ADR 0015).
- `--public` no-auth aggregate dashboard is documented and tested (`PUBLIC_READ_ROUTE_INVENTORY`).
- SSH remote argv is not passed through a local shell (`split_remote_command`).
- Main dashboard `innerHTML` interpolations reviewed use `escapeHtml` for event/model/project strings.
- `JobRegistry` mutex `expect` on poison is a process-already-broken path.
- GitHub path `bahayonghang/llmuasage` is the real repository name, not a typo in `update.rs`.
- `TODO.md` is complete. No TODO/FIXME in first-party `src/`.

## Open questions

- Whether loopback CSRF should be a local token, Origin/Host allowlist, or both. Child `loopback-write-csrf` recommends Origin/Host first.
- Whether `codex_tracer` stays a sidecar forever. Hygiene documents it; merge is out of scope for this parent.
- Whether `CONTEXT.md` should be restored. Domain docs say producers create it lazily; ADRs still link it.
