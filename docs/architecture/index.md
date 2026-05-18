# Architecture

This page explains the current 0.6.x shape. For decision records, see [ADR](../adr/). For historical product plans, see the [PRD archive](../prd/).

## Runtime layout

The runtime state lives under `~/.llmusage/` unless overridden by `--home <PATH>` or `LLMUSAGE_HOME`.

- `llmusage.db` stores schema metadata, cursors, events, 30-minute buckets, behavior facts, project metadata, source-file diagnostics, integration state, trigger state, pricing metadata, worker lock metadata, and run logs.
- `bin/llmusage-hook.cmd` and `bin/llmusage-hook.sh` are local wrappers called by external tools.
- `exports/` stores static HTML reports.
- `backups/` stores integration config backups used by uninstall.
- `pricing/` stores local pricing snapshots imported by `doctor --refresh-pricing`.

## Source registry

`SourceKind` currently includes Codex, Claude, OpenCode, and Gemini. The registry is the single fan-out point for parsers and integrations:

- `registered_parsers()` powers `llmusage sync`.
- `registered_integrations()` powers `init`, `doctor`, and `uninstall`-style integration flows.

Adding a source means adding a `SourceKind` variant plus a registered `SourceParser` and `Integration`.

## Sync flow

1. A tool-specific hook or plugin triggers `llmusage hook-run`, or the user runs `llmusage sync`.
2. The command bootstraps/migrates SQLite and acquires the local `worker_lock`.
3. The driver walks registered parsers in source order: Codex, Claude, OpenCode, Gemini.
4. Each parser emits `SyncShard` values.
5. `SyncRunWriter::commit_shard` performs reset, event write, cursor write, raw archive write, behavior fact write, and source-file stamping as the commit protocol.
6. The store saves per-source sync status and run-log records.

`SyncShard` is the parser/writer boundary. Parsers do not write SQLite directly.

## Query and dashboard flow

Report commands, TUI, web dashboard, and HTML export all read local SQLite through the query layer.

`Dashboard::snapshot(&QueryFilter)` is the primary dashboard seam. `llmusage serve` prefers `/api/dashboard` so overview, trend series, model/source/project/cost rankings, health, and diagnostics are loaded from one core snapshot. Activity, Tools, Optimize, and Compare are behavior sections that may degrade independently when source facts are unavailable or queries time out.

## Behavior facts

The 0.6.x line adds normalized behavior tables:

- `usage_turn`: turn-level facts for Activity, Optimize, and Compare.
- `usage_tool_call`: bounded tool/action facts for Tools, Optimize, and Compare.

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
- Gemini registration,
- `pricing_catalog_version`,
- behavior fact tables,
- compatibility repair for historical `source_sync_status.stored_events` drift.

`schema_version` alone is not treated as a complete safety proof; compatibility repairs can be idempotent migrations when deployed databases drift.

## Local-only guarantees

- No device token.
- No account login.
- No upload queue.
- No remote usage API call.
- Pricing refresh reads a user-provided local JSON file.
- Browser dashboard binds to `127.0.0.1`.
