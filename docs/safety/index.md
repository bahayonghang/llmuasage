# Safety

`llmusage` is designed around a local-first boundary. This page lists the data paths and the commands that can write or delete local state.

## Local data paths

Default runtime root:

```text
~/.llmusage/
```

Common files and directories:

| Path | Purpose |
| --- | --- |
| `~/.llmusage/llmusage.db` | SQLite database for usage, buckets, cursors, diagnostics, jobs, run logs, and metadata |
| `~/.llmusage/bin/llmusage-hook.cmd` | Windows hook wrapper |
| `~/.llmusage/bin/llmusage-hook.sh` | POSIX hook wrapper |
| `~/.llmusage/backups/` | Integration config backups used by uninstall |
| `~/.llmusage/exports/` | Static HTML exports |
| `~/.llmusage/logs/llmusage.ndjson` | Local structured runtime diagnostics and command tracing |
| `~/.llmusage/pricing/` | Content-addressed local base, overlay, and effective pricing catalogs |

Runtime root precedence: `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.

## What is not uploaded

`llmusage` does not create an account session, device token, upload queue, or remote usage API call. Report, dashboard, and export surfaces read local SQLite.

Project labels are derived locally. Sensitive path dimensions are stored as hashes where the schema needs stable grouping.

Runtime diagnostics stay local. `LLMUSAGE_LOG` controls the NDJSON log file (`off`, `error`, `warn`, `info`, `debug`, or `trace`; default `warn`), while `RUST_LOG` controls console stderr. Runtime log events include command labels, run ids, sources, module targets, and error summaries; they do not intentionally record prompts, responses, or raw source JSON. Paths may appear in human error summaries, so treat diagnostics bundles as local troubleshooting artifacts.

Use `llmusage logs --limit 50 --level warn` to query recent runtime log entries and SQLite `run_log` records. The command reads only local files/database rows and does not upload anything. The active log file is capped conservatively by rotating to `llmusage.ndjson.old` when it is over 10 MiB on startup; delete old local logs manually if you no longer need them.

## Normal sync is retention-safe

```powershell
llmusage sync
```

Normal sync imports new/changed local source artifacts. If a file-backed source that was previously imported is now missing, sync keeps imported usage history and marks the source file as missing for diagnostics.

## Rebuild can be destructive

```powershell
llmusage sync --rebuild
```

`--rebuild` resets parser-backed usage state source by source before reparsing local sources. A full rebuild uses the parser registry as its deletion boundary, so parserless Antigravity events, buckets, behavior facts, cursors, and source-file diagnostics are preserved. If imported file-backed history for a parser source depends on files that are now missing, llmusage refuses the rebuild before any reset.

The explicit override is:

```powershell
llmusage sync --rebuild --allow-lossy-rebuild
```

Use it only when you accept clearing unrebuildable imported history.

## Dashboard startup migration

`llmusage serve` checks for legacy parser-backed token accounting before it
binds a local port. It automatically rebuilds only sources whose tracked input
files are still available. A source with lossy rebuild risk is skipped with a
warning; its history remains readable, normal writes remain guarded, and the
dashboard continues to start. Unexpected failures after a source passes the
safety check stop startup.

This startup path never enables `--allow-lossy-rebuild`. Parserless sources are
not migration targets.

## Diagnose missing source files

```powershell
llmusage diagnostics --out .\llmusage-diagnostics.json
```

Diagnostics include source-file archive state such as missing file count, protected event count, and lossy rebuild risk.

If a source file should be intentionally ignored, use the explicit write path:

```powershell
llmusage diagnostics --forget-file <PATH> --source codex
```

This marks the row as `deleted_by_user` and removes its cursor row.

## Pricing catalog changes are local-file only

```powershell
llmusage catalog apply .\pricing-overlay.json
llmusage catalog status --json
llmusage catalog reset
llmusage doctor --refresh-pricing .\litellm-prices.json
```

`catalog apply` activates an incremental v2 overlay. `doctor --refresh-pricing` activates a complete base snapshot and clears any overlay. Both commands accept only existing local files; URLs and remote fetching are refused.

Activation writes SHA-256-addressed files under `~/.llmusage/pricing/`, recomputes local event and bucket costs, and then switches SQLite catalog metadata. A missing, modified, or invalid selected file is an explicit error; llmusage does not silently fall back to embedded prices. `catalog reset` removes an overlay and recomputes costs with its recorded base. Unreferenced digest files may remain as local audit artifacts and are removed by `uninstall --purge` with the rest of the runtime root.

## Browser dashboard boundary

`llmusage serve` binds to `127.0.0.1` only. It exposes local HTTP endpoints for the dashboard while the process is running. It does not open a public listener.

## Static export boundary

`llmusage export html` writes a static snapshot directory. Share it only if you are comfortable sharing the aggregated usage values and labels captured in `snapshot.json`.
