# First sync

`llmusage sync` imports local usage into SQLite. Report commands do not auto-sync, so run sync when the database is stale.

## Import all sources

```powershell
llmusage sync
```

Human progress is written to stderr. The final summary stays on stdout.

## Import one source

```powershell
llmusage sync --source codex
llmusage sync --source claude
llmusage sync --source opencode
llmusage sync --source gemini
```

The accepted source values match `cargo run -- --help`: `codex`, `claude`, `opencode`, and `gemini`.

## Emit NDJSON progress

```powershell
llmusage sync --json-events
```

This mode prints lifecycle and progress events as NDJSON on stdout. Use it for wrappers or UI adapters that need machine-readable progress.

## Recent-ready signal

```powershell
llmusage sync --recent-days 1
```

`--recent-days` enables recent-window signalling for callers. The current parser surface still scans existing cursors as needed to preserve correctness.

## Rebuild safely

```powershell
llmusage sync --rebuild
```

`--rebuild` clears rebuildable usage rows, buckets, projects, and cursors before reparsing local sources. It is refused by default when file-backed imported history depends on source files that are now missing.

Only pass the lossy flag when you intentionally accept clearing unrebuildable history:

```powershell
llmusage sync --rebuild --allow-lossy-rebuild
```

For a safer diagnosis first:

```powershell
llmusage diagnostics --out .\llmusage-diagnostics.json
```

## What sync writes

- `usage_event`: normalized source events.
- `usage_bucket_30m`: 30-minute UTC aggregates used by reports and dashboards.
- `usage_turn` and `usage_tool_call`: privacy-bounded behavior facts.
- `source_file`: live/missing/deleted source-file state for diagnostics.
- `source_cursor`: incremental cursors.
- `run_log` and `source_sync_status`: operational status.
