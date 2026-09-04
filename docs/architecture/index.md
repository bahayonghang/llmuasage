# Architecture

This page explains the current architecture shape. For decision records, see [ADR](../adr/). For historical product plans, see the [PRD archive](../prd/).

## Runtime layout

The runtime state lives under `~/.llmusage/` unless overridden by `--home <PATH>` or `LLMUSAGE_HOME`.

- `llmusage.db` stores schema metadata, cursors, events, 30-minute buckets, behavior facts, project metadata, source-file diagnostics, pricing metadata, worker lock metadata, and run logs. Historical `trigger_state` and `integration_install` tables remain for database compatibility; only actual legacy cleanup can append integration audit rows.
- `codex-tracer.db` is the independent SQLite file for `llmusage codex-tracer`. It is not a `SyncShard` target and is not merged into the main `Store`.
- `exports/` stores static HTML reports.
- `backups/` retains historical integration config backups and pricing/database recovery material.
- `pricing/` stores content-addressed base, overlay, and effective pricing catalogs activated by `catalog` or `doctor --refresh-pricing`.

## Source registry

`SourceKind` currently includes Codex, Claude, OpenCode, Antigravity, Kimi Code, Pi, Oh My Pi, and Grok Build. `antigravity`, `kimi_code`, `pi`, `omp`, and `grok` are stable CLI/API/SQLite source ids; `gemini-*` strings remain model ids only.

`SourceDescriptor` is the source capability registry. It declares each source's stable id, aliases, parser/passive-probe capabilities, token-quality label, and local privacy boundary. The registry is the single fan-out point for parsers and source descriptors:

- `registered_parsers()` powers `llmusage sync`.
- `registered_source_descriptors()` powers capability/status semantics and guards parser drift.

Adding a source means adding a `SourceKind` variant plus a descriptor. A parser is added only when the descriptor's capability declaration and tests justify it. Passive readers also require real local samples, fixture coverage, sync-twice idempotency, cursor/rebuild behavior, token-quality declaration, and privacy review before they can write usage rows. Antigravity is parser-backed for CLI `conversations/*.db`; hook-era rows remain queryable and a rebuild is refused while they have no file attribution.

`PlatformMonitorDescriptor` is the wider monitoring catalog. It describes registered passive sources such as Kimi Code, Pi, Oh My Pi, Grok Build, ZCode, Antigravity CLI, and DeepSeek Harness alongside parserless candidates such as Reasonix, Gemini CLI, Cursor, Copilot, Zed, Kiro, Goose, Kimi shell/Qwen, Roo/Kilo/Cline, Codebuff, Crush, Warp/Oz, Amp, Hermes, and Trae. Monitor descriptors may surface detected/unavailable roots, parser support, privacy class, token quality, and next action in `source-status` and `dash`; only descriptors backed by a registered `SourceKind` and parser can write usage rows.

## Sync flow

1. The user or an in-process dashboard job runs `llmusage sync`.
2. The command bootstraps/migrates SQLite and acquires the local `worker_lock`.
3. Sync walks registered passive parsers in source order: Codex, Claude, OpenCode, Kimi Code, Pi, Oh My Pi, and Grok Build. Antigravity is parser-backed for CLI `conversations/*.db`; hook-era rows remain visible.
4. Each parser emits `SyncShard` values.
5. `SyncRunWriter::commit_shard` performs reset, event write, cursor write, raw archive write, behavior fact write, and source-file stamping as the commit protocol.
6. The store saves per-source sync status and run-log records.

`SyncShard` is the parser/writer boundary. Parsers do not write SQLite directly.

Repeated sync work is avoided through per-source cursors. Codex, Claude, Kimi Code, Pi, Oh My Pi, and Grok Build compare file size, mtime, head fingerprint, tail signature, and offset before reparsing; OpenCode compares DB identity and message high-water cursors. Kimi imports only turn-scoped `usage.record` rows. Pi reads `~/.pi/agent/sessions` (or `PI_AGENT_DIR`) as `pi`; Oh My Pi reads `~/.omp/agent/sessions` as `omp`. They share one parse implementation. Overlapping paths belong to `pi`. Both keep upstream totals authoritative, with reasoning as a separate diagnostic channel. Grok scans only direct session sidecars, replays the full session when any sidecar changes, maps `turn_completed.usage` as precise events (total-only fallback when usage is absent), and keeps cost unpriced. Sync stats expose unchanged work as skipped, changed artifacts as parsed, newly inserted rows as committed, and durable totals as stored events.

## Codex Tracer sidecar

`llmusage codex-tracer` is a sidecar product island. It reads Codex JSONL files and writes `codex-tracer.db` under the same runtime root. It does not emit `SyncShard` values and does not share the main `llmusage.db` usage import path. Do not merge it into `Store`.

## Query and dashboard flow

Report commands, TUI (`llmusage dash`; the hidden `tui` command is a deprecated alias for `dash`), web dashboard, and HTML export all read local SQLite through the query layer.

TUI Usage may send a read-only quota request to a vendor endpoint when a local credential file already exists (Claude, Codex, Grok Build, Kimi). The fetchers do not refresh tokens, rewrite credential files, create an account, or upload usage events.

`Dashboard::snapshot(&QueryFilter)` is the primary dashboard seam. `llmusage serve` prefers `/api/dashboard` so overview, trend series, model/source/project/cost rankings, health, diagnostics, and the default Usage analysis payload are loaded from one core snapshot. Activity categories (`activity`), Tool usage (`tools`), Optimization hints (`optimize`), Usage analysis (`explorer`), and Model comparison (`compare`) may degrade independently when source facts are unavailable or queries time out.

Custom Usage analysis queries use `Dashboard::explorer(&ExplorerQuery)` and the `/api/explorer` endpoint. Usage analysis is additive to the fixed dashboard snapshot: it supports time granularity, metric, group-by, result limits/Other, and session/tool/token filters, but it still returns backend-aggregated rows and series rather than asking the browser to pivot raw events. Query execution chooses an event, turn, or tool-attribution strategy based on the selected metric and dimension, and every payload carries support metadata such as `normalized`, `no_data`, `degraded`, or `unsupported`.

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

The current version adds normalized behavior tables:

- `usage_turn`: turn-level facts for Activity categories, Optimization hints, Model comparison, and turn-backed Usage analysis queries.
- `usage_tool_call`: bounded tool/action facts for Tool usage, Optimization hints, Model comparison, and tool-attribution Usage analysis queries.

Privacy boundary: behavior facts must not store full prompts, full assistant text, or file contents. `safe_preview` is bounded display text only.

## Store façade

`Store` is a façade for paths, connections, worker locks, bootstrap, rebuild/reset, and sync writer creation. Domain stores are exposed as borrowed views such as `CursorStore`, `HostStore`, `RunLog`, `SyncStatusStore`, and `SourceFileStore`. There is no active hook worker or trigger-state write API.

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
- Usage events, reports, the browser dashboard, and HTML export stay on local SQLite. llmusage does not upload usage data.
- TUI Usage (`llmusage dash`) may read-only fetch vendor quota with credentials already stored on the machine. That path is quota display only. It is not an account login and not a usage-upload API.
- Pricing catalog activation reads user-provided local JSON files and never fetches remote pricing.
- Browser dashboard binds to `127.0.0.1` by default; `serve --public` explicitly changes the listener to `0.0.0.0` without adding authentication or TLS.
