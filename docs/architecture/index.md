# Architecture

This page explains the current 0.6.x shape. For decision records, see [ADR](../adr/). For historical product plans, see the [PRD archive](../prd/).

## Runtime layout

The runtime state lives under `~/.llmusage/` unless overridden by `--home <PATH>` or `LLMUSAGE_HOME`.

- `llmusage.db` stores schema metadata, cursors, events, 30-minute buckets, behavior facts, project metadata, source-file diagnostics, integration state, trigger state, pricing metadata, worker lock metadata, and run logs.
- `bin/llmusage-hook.cmd` and `bin/llmusage-hook.sh` are local wrappers called by external tools.
- `exports/` stores static HTML reports.
- `backups/` stores integration config backups used by uninstall.
- `pricing/` stores content-addressed base, overlay, and effective pricing catalogs activated by `catalog` or `doctor --refresh-pricing`.

## Source registry

`SourceKind` currently includes Codex, Claude, OpenCode, and Antigravity. `antigravity` is the stable CLI/API/SQLite source id; `gemini-*` strings remain model ids only.

`SourceDescriptor` is the source capability registry. It declares each source's stable id, aliases, activation mode (`hook`, `plugin`, `passive`, or `hybrid`), parser/integration capabilities, token-quality label, and local privacy boundary. The registry is the single fan-out point for parsers, integrations, and source descriptors:

- `registered_parsers()` powers `llmusage sync`.
- `registered_integrations()` powers `init`, `doctor`, and `uninstall`-style integration flows.
- `registered_source_descriptors()` powers capability/status semantics and guards parser/integration drift.

Adding a source means adding a `SourceKind` variant plus a descriptor. A parser or integration is added only when the descriptor's capability declaration and tests justify it. Passive readers also require real local samples, fixture coverage, sync-twice idempotency, cursor/rebuild behavior, token-quality declaration, and privacy review before they can write usage rows.

`PlatformMonitorDescriptor` is the wider monitoring catalog. It can describe parserless candidates from tokscale-style evidence, including Gemini CLI, Cursor, Copilot, Zed, Kiro, Goose, Grok, Kimi/Qwen, Roo/Kilo/Cline, Codebuff, Crush, Warp/Oz, Amp, Hermes, and Trae. Monitor descriptors may surface detected/unavailable roots, parser support, privacy class, token quality, and next action in `source-status` and `dash`, but they are not persisted as `SourceKind` and cannot write usage rows.

## Sync flow

1. A tool-specific hook or plugin triggers `llmusage hook-run`, or the user runs `llmusage sync`.
2. The command bootstraps/migrates SQLite and acquires the local `worker_lock`.
3. A manual sync walks registered parsers in source order: Codex, Claude, and OpenCode. Antigravity is hook/integration-only until a verified transcript schema exists. A hook-run sync is filtered to the triggering source so one hook does not import every parser-backed source.
4. Each parser emits `SyncShard` values.
5. `SyncRunWriter::commit_shard` performs reset, event write, cursor write, raw archive write, behavior fact write, and source-file stamping as the commit protocol.
6. The store saves per-source sync status and run-log records.

Codex `notify` is a singleton integration. llmusage backs up a distinct original notify during install and chains it best-effort after llmusage hook handling, skipping recursive/self commands and never blocking hook success on the chained command.

`SyncShard` is the parser/writer boundary. Parsers do not write SQLite directly.

Repeated sync work is avoided through per-source cursors. Codex and Claude compare file size, mtime, head fingerprint, tail signature, and offset before reparsing; OpenCode compares DB identity and message high-water cursors. Sync stats expose unchanged work as skipped, changed artifacts as parsed, newly inserted rows as committed, and durable totals as stored events.

## Query and dashboard flow

Report commands, TUI, web dashboard, and HTML export all read local SQLite through the query layer.

`Dashboard::snapshot(&QueryFilter)` is the primary dashboard seam. `llmusage serve` prefers `/api/dashboard` so overview, trend series, model/source/project/cost rankings, health, diagnostics, and the default Explorer payload are loaded from one core snapshot. Activity, Tools, Optimize, Explorer, and Compare are behavior/query sections that may degrade independently when source facts are unavailable or queries time out.

Custom Cost Explorer queries use `Dashboard::explorer(&ExplorerQuery)` and the `/api/explorer` endpoint. Explorer is additive to the fixed dashboard snapshot: it supports time granularity, metric, group-by, Top N/Other, and session/tool/token filters, but it still returns backend-aggregated rows and series rather than asking the browser to pivot raw events. Query execution chooses an event, turn, or tool-attribution strategy based on the selected metric and dimension, and every payload carries support metadata such as `normalized`, `no_data`, `degraded`, or `unsupported`.

## Pricing catalog flow

The embedded `pricing/static-v2.json` file is the default base. It owns stable model ids, source expansion, exact/family matchers, default and threshold rates, and context windows. Parser-owned model strings remain unchanged in SQLite.

The optional user layer is a v2 `overlay` activated explicitly with `catalog apply`. Merge is deterministic: validate the base, apply strict `remove_models`, replace or append complete definitions by stable id, then validate the effective catalog. There is no field-level deep merge. Exact matchers win over family matchers, then longer matchers win.

Catalog files under `~/.llmusage/pricing/` use SHA-256 names:

- `base-<sha256>.json` for a pinned base copy,
- `overlays/overlay-<sha256>.json` for the user overlay,
- `effective-<sha256>.json` for the normalized merged catalog.

SQLite meta records the active, base, and overlay identities/files. A selected file that is missing, modified, or invalid fails closed instead of falling back to embedded data. Applying a catalog writes and validates files first, recomputes each `usage_event`, reconciles 30-minute bucket pricing, and switches all catalog metadata in the final transaction. `doctor --refresh-pricing` uses the same activation service for a complete base snapshot.

Threshold rates are selected per event from input + cache-read + cache-creation tokens. Buckets and reports only sum persisted event costs. Context-pressure queries load the active catalog, so configured models use their configured context windows.

## Behavior facts

The 0.6.x line adds normalized behavior tables:

- `usage_turn`: turn-level facts for Activity, Optimize, Compare, and turn-backed Explorer queries.
- `usage_tool_call`: bounded tool/action facts for Tools, Optimize, Compare, and tool-attribution Explorer queries.

Privacy boundary: behavior facts must not store full prompts, full assistant text, or file contents. `safe_preview` is bounded display text only.

## Store façade

`Store` is a façade for paths, connections, worker locks, bootstrap, rebuild/reset, and sync writer creation. Domain stores are exposed as borrowed views such as `CursorStore`, `RunLog`, `SyncStatusStore`, `TriggerStore`, and `SourceFileStore`.

## JobRegistry

`JobRegistry` is an in-process sync job registry for library/web adapters. It provides start/get/cancel snapshots, but it is not durable across process restarts. Durable recovery remains in SQLite usage rows, cursors, source-file diagnostics, and run logs.

## Schema migrations

Schema migrations are explicit and versioned. The current line includes:

- baseline migrations for the original usage tables,
- cache/cost/pricing metadata,
- source-file state,
- raw archive opt-in,
- worker lock metadata,
- Antigravity source registration,
- active/base/overlay pricing catalog metadata,
- behavior fact tables,
- v13 `gemini` → `antigravity` source-id cutover,
- compatibility repair for historical `source_sync_status.stored_events` drift.

`schema_version` alone is not treated as a complete safety proof; compatibility repairs can be idempotent migrations when deployed databases drift.

## Local-only guarantees

- No device token.
- No account login.
- No upload queue.
- No remote usage API call.
- Pricing catalog activation reads user-provided local JSON files and never fetches remote pricing.
- Browser dashboard binds to `127.0.0.1` by default; `serve --public` explicitly changes the listener to `0.0.0.0` without adding authentication or TLS.
