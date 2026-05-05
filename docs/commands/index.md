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
- `--source codex|claude|opencode`

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

Creates the local runtime, bootstraps SQLite, writes hook wrappers, and installs the supported integrations.

### `llmusage sync`

Runs the local parsers for Codex, Claude, and OpenCode, then updates the 30-minute buckets. Use `--rebuild` to clear rebuildable usage rows, buckets, projects, and cursors before reparsing local sources.

### `llmusage status`

Prints a human-readable summary: DB path, buckets, last sync, source totals, integration state, and recent errors.

### `llmusage diagnostics`

Emits machine-readable JSON for paths, integrations, SQLite state, cursors, source totals, health checks, and recent runs.

### `llmusage doctor`

Runs read-only health checks over wrapper presence, integration drift, OpenCode DB presence, and recent failures.

## Local UI commands

### `llmusage serve`

Starts the local dashboard and JSON API on `127.0.0.1`.

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
