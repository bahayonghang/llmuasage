# Command Reference

## Core commands

### `llmusage init`

Creates the local runtime, bootstraps SQLite, writes hook wrappers, and installs the supported integrations.

### `llmusage sync`

Runs the local parsers for Codex, Claude, and OpenCode, then updates the 30-minute buckets.

### `llmusage status`

Prints a human-readable summary: DB path, buckets, last sync, source totals, integration state, and recent errors.

### `llmusage diagnostics`

Emits machine-readable JSON for paths, integrations, SQLite state, cursors, source totals, health checks, and recent runs.

### `llmusage doctor`

Runs read-only health checks over wrapper presence, integration drift, OpenCode DB presence, and recent failures.

## Local UI commands

### `llmusage serve`

Starts the local dashboard and JSON API on `127.0.0.1`.

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
