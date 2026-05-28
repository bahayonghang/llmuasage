# CLI reference

This page is aligned with `cargo run -- --help`, `cargo run -- serve --help`, and `cargo run -- export html --help` for version `0.6.5`. Top-level help is rendered as a compact table; command-specific help still uses the clap output.

## Top-level help

```powershell
llmusage help
llmusage --help
llmusage -h
llmusage help --zh
```

`llmusage help`, `llmusage --help`, and `llmusage -h` print the English table help. `llmusage help --zh` prints the Chinese table help. Use `llmusage help <COMMAND>` or `llmusage <COMMAND> --help` for the legacy command-specific clap help.

## Global options

```text
Usage: llmusage [OPTIONS] [COMMAND]
```

| Option | Meaning |
| --- | --- |
| `--home <PATH>` | Override `LLMUSAGE_HOME` and the default `~/.llmusage` runtime root |
| `--since <YYYYMMDD>` | Inclusive start date for report commands |
| `--until <YYYYMMDD>` | Inclusive end date for report commands |
| `--json` | Emit stable JSON for supported report commands |
| `--breakdown` | Include per-model breakdown rows or payloads where supported |
| `--order asc\|desc` | Sort report rows by period/activity |
| `--timezone UTC\|local\|+08:00` | Report timezone |
| `--locale <LOCALE>` | Lightweight locale selector for titles and number formatting |
| `--compact` | Use a narrower table layout |
| `--source codex\|claude\|opencode\|gemini` | Restrict reports or sync to one source; `antigravity` is accepted as an alias for `gemini` |
| `--all` | Show full daily history instead of the default last 7 days |
| `--instances` | Group daily rows by project/instance |
| `--project <PROJECT>` | Filter by project label, hash, or reference |

## Report commands

Report commands read the local database only.

### `llmusage` / `llmusage daily`

```powershell
llmusage
llmusage daily --all
llmusage daily --source codex --since 20260501 --until 20260518
llmusage daily --json --breakdown
```

Default command. Shows daily token and estimated-cost usage.

### `llmusage monthly`

```powershell
llmusage monthly --breakdown
```

Groups usage by month.

### `llmusage session`

```powershell
llmusage session
llmusage session --id <ID>
llmusage session --project my-repo
```

Groups usage by source session. `--id <ID>` accepts an exact or partial session id.

### `llmusage blocks`

```powershell
llmusage blocks --active
llmusage blocks --recent
llmusage blocks --token-limit max
llmusage blocks --session-length 5
```

Shows 5-hour usage blocks and burn-rate projections.

### `llmusage statusline`

```powershell
llmusage statusline
llmusage statusline --no-cache
llmusage statusline --refresh-interval 10 --cost-source llmusage
```

Prints one statusline-friendly usage summary.

## Setup and sync commands

### `llmusage init`

```powershell
llmusage init
```

Creates the local runtime and installs/probes integrations.

### `llmusage sync`

```powershell
llmusage sync
llmusage sync --source gemini
llmusage sync --source antigravity
llmusage sync --recent-days 1
llmusage sync --json-events
llmusage sync --rebuild
llmusage sync --rebuild --allow-lossy-rebuild
```

Imports local sources. `--json-events` writes NDJSON lifecycle events to stdout. `--allow-lossy-rebuild` requires `--rebuild`.

## Status and diagnostics

### `llmusage status`

```powershell
llmusage status
```

Prints a human-readable database, source, integration, and recent-run summary.

### `llmusage diagnostics`

```powershell
llmusage diagnostics
llmusage diagnostics --out .\llmusage-diagnostics.json
llmusage diagnostics --forget-file <PATH> --source codex
```

Emits machine-readable diagnostics. `--forget-file` marks a source file as `deleted_by_user` and removes its cursor row.

### `llmusage doctor`

```powershell
llmusage doctor
llmusage doctor --json
llmusage doctor --refresh-pricing .\litellm-prices.json
```

Runs health checks. `--refresh-pricing <PATH>` copies a local LiteLLM pricing snapshot into `~/.llmusage/pricing/` and recomputes per-event costs. URLs are refused.

## Local UI commands

### `llmusage dash`

```powershell
llmusage dash
```

Interactive terminal dashboard. The old hidden `tui` command is a deprecated alias.

### `llmusage serve`

```powershell
llmusage serve
llmusage serve --port 37421
```

Starts the local web dashboard and JSON API on `127.0.0.1`.

### `llmusage export html`

```powershell
llmusage export html
llmusage export html --out .\llmusage-report
```

Writes a static dashboard bundle.

## Removal

### `llmusage uninstall`

```powershell
llmusage uninstall
llmusage uninstall --purge
```

Restores modified integration files. `--purge` also removes the runtime root.

## Hidden command

`hook-run` is intentionally hidden from normal help. It is called by generated hook wrappers.
