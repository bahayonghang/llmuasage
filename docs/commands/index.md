# Command Reference

## Report commands

Report commands read `~/.llmusage/llmusage.db` only. They do **not** trigger `sync`, so run `llmusage sync` first when the local database is stale.

### `llmusage` / `llmusage daily`

Shows token and estimated-cost totals for the last 7 calendar days in the selected timezone, including today. Estimated costs read the persisted cache-aware `cost_with_cache_usd` column. With no subcommand, `llmusage` is equivalent to `llmusage daily`; use `--all` for full daily history.

Human output is aggregate-first: all matching sources render into one ccusage-style daily table headed `LLM Usage Report - Daily`. The default columns are `Date / Models / Input / Output / Cache Create / Cache Read / Total Tokens / Cost (USD)`, token counts use full comma grouping on wide terminals, models render as multi-line `- model` lists, and `NO_COLOR=1` disables ANSI styling. Source separation is available through `--source` filtering and `--breakdown` per-source/model rows. `--json` stays the stable aggregate snake_case payload and includes `cache_creation_tokens`.

Useful options:

- `--since YYYYMMDD` / `--until YYYYMMDD`
- `--all` to show full daily history instead of the default last 7 days
- `--json`
- `--breakdown`
- `--instances` to group daily rows by project
- `--project <label|hash|ref>`
- `--timezone UTC|local|+08:00`
- `--compact`
- `--source codex|claude|opencode|gemini`

### `llmusage monthly`

Groups the same local usage rows by month and supports JSON, breakdown, date range, timezone, compact layout, and source filtering.

### `llmusage session`

Groups usage by source session. Use `--id <session_id>` to inspect one session and `--project` to restrict the list to a project. Older databases without session metadata use a stable source-file fallback; run `llmusage sync --rebuild` to repopulate session ids from local sources only while those source files are still present.

### `llmusage blocks`

Builds 5-hour usage windows for burn-rate style views.

Options include:

- `--active`
- `--recent`
- `--token-limit <number|max>`
- `--session-length <hours>`

### `llmusage statusline`

Prints one hook/status-bar friendly line using the local DB. It reads hook JSON from stdin when present and writes a small cache under `~/.llmusage/statusline-cache/` unless `--no-cache` is set.

## Core commands

### `llmusage init`

Creates the local runtime, bootstraps SQLite, writes hook wrappers, and installs the supported Codex / Claude / OpenCode / Gemini integrations.

### `llmusage sync`

Runs the local parsers for Codex, Claude, OpenCode, and Gemini, then updates the 30-minute buckets including persisted cost/pricing rollups. Use `--source codex|claude|opencode|gemini` to restrict the run, and `--rebuild` to clear rebuildable usage rows, buckets, projects, and cursors before reparsing local sources. Before deleting usage, rebuild preflights file-backed sources: if imported events depend on source files that are now missing, the command is refused by default. Regular `llmusage sync` is safe in that state; it marks source files as missing for diagnostics but keeps usage history. Pass `--allow-lossy-rebuild` with `--rebuild` only when you intentionally accept clearing unrebuildable history. Default progress is written to stderr so stdout keeps the final summary; `--json-events` instead emits NDJSON lifecycle/progress events on stdout.

### `llmusage status`

Prints a human-readable summary: DB path, buckets, last sync, source totals, integration state, and recent errors.

### `llmusage diagnostics`

Emits machine-readable JSON for paths, integrations, SQLite state, cursors, source totals, source-file archive diagnostics, health checks, and recent runs. Source archive rows include `missing_file_count`, `protected_event_count`, and `lossy_rebuild_risk` so callers can distinguish missing raw source files from lost imported usage. `--forget-file <PATH>` marks a source file as intentionally ignored; use `--source` when the same path exists under multiple sources.

### `llmusage doctor`

Runs read-only health checks over wrapper presence, integration drift, local source presence, and recent failures. `--refresh-pricing <file>` is the one write mode: it imports a local pricing JSON snapshot, stores it as `~/.llmusage/pricing/<catalog-version>.json`, recomputes event and bucket costs, and records `pricing_catalog_version`.

## Local UI commands

### `llmusage serve`

Starts the local dashboard and JSON API on `127.0.0.1`, then opens the dashboard in the default browser. The dashboard uses `/api/dashboard` for an initial single-connection snapshot and keeps overview, trends, model/source/project/cost rankings, health, and diagnostics on the same `source` / `model` / `since` / `until` / `window` filter. Live mode also exposes JSON export, optional 30s/60s auto-refresh, and sync job start/cancel/progress controls.

![llmusage web dashboard overview](/screenshots/web-dashboard-overview.png)

### `llmusage tui`

Opens the terminal summary panel.

### `llmusage export html`

Writes a static bundle:

- `index.html`
- `snapshot.json`
- `assets/*`

The exported bundle reuses the same dashboard shell and embeds the captured filter in `snapshot.json`; live-only sync and auto-refresh controls are disabled with an explanation.

## Removal

### `llmusage uninstall`

Restores modified integration files. Use `--purge` only if you also want to remove `~/.llmusage/`.
