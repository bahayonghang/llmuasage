# Command Reference

## Report commands

Report commands read `~/.llmusage/llmusage.db` only. They do **not** trigger `sync`, so run `llmusage sync` first when the local database is stale.

### `llmusage` / `llmusage daily`

Shows today's token and estimated-cost totals in the selected timezone. With no subcommand, `llmusage` is equivalent to `llmusage daily`; use `--all` for full daily history.

Useful options:

- `--since YYYYMMDD` / `--until YYYYMMDD`
- `--all`
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

Groups usage by source session. Use `--id <session_id>` to inspect one session and `--project` to restrict the list to a project. Older databases without session metadata use a stable source-file fallback; run `llmusage sync --rebuild` to repopulate session ids from local sources.

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

Runs the local parsers for Codex, Claude, OpenCode, and Gemini, then updates the 30-minute buckets. Use `--source codex|claude|opencode|gemini` to restrict the run, and `--rebuild` to clear rebuildable usage rows, buckets, projects, and cursors before reparsing local sources. Default progress is written to stderr so stdout keeps the final summary; `--json-events` instead emits NDJSON lifecycle/progress events on stdout.

### `llmusage status`

Prints a human-readable summary: DB path, buckets, last sync, source totals, integration state, and recent errors.

### `llmusage diagnostics`

Emits machine-readable JSON for paths, integrations, SQLite state, cursors, source totals, source-file archive diagnostics, health checks, and recent runs. `--forget-file <PATH>` marks a source file as intentionally ignored; use `--source` when the same path exists under multiple sources.

### `llmusage doctor`

Runs read-only health checks over wrapper presence, integration drift, local source presence, and recent failures. `--refresh-pricing <file>` is the one write mode: it imports a local pricing JSON snapshot, recomputes local costs, and records `pricing_catalog_version`.

## Local UI commands

### `llmusage serve`

Starts the local dashboard and JSON API on `127.0.0.1`, then opens the dashboard in the default browser.

![llmusage web dashboard overview](/screenshots/web-dashboard-overview.png)

### `llmusage tui`

Opens the terminal summary panel.

### `llmusage export html`

Writes a static bundle:

- `index.html`
- `snapshot.json`
- `assets/*`

## Removal

### `llmusage uninstall`

Restores modified integration files. Use `--purge` only if you also want to remove `~/.llmusage/`.
