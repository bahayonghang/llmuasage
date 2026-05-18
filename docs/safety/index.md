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
| `~/.llmusage/pricing/` | Local pricing snapshots imported with `doctor --refresh-pricing` |

Runtime root precedence: `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.

## What is not uploaded

`llmusage` does not create an account session, device token, upload queue, or remote usage API call. Report, dashboard, and export surfaces read local SQLite.

Project labels are derived locally. Sensitive path dimensions are stored as hashes where the schema needs stable grouping.

## Normal sync is retention-safe

```powershell
llmusage sync
```

Normal sync imports new/changed local source artifacts. If a file-backed source that was previously imported is now missing, sync keeps imported usage history and marks the source file as missing for diagnostics.

## Rebuild can be destructive

```powershell
llmusage sync --rebuild
```

`--rebuild` deletes rebuildable usage rows, buckets, project rows, and cursors before reparsing local sources. If imported file-backed history depends on source files that are now missing, llmusage refuses the rebuild by default.

The explicit override is:

```powershell
llmusage sync --rebuild --allow-lossy-rebuild
```

Use it only when you accept clearing unrebuildable imported history.

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

## Pricing refresh is local-file only

```powershell
llmusage doctor --refresh-pricing .\litellm-prices.json
```

This copies the local JSON snapshot under `~/.llmusage/pricing/`, recomputes local cost columns, and records `pricing_catalog_version`. URLs are refused.

## Browser dashboard boundary

`llmusage serve` binds to `127.0.0.1` only. It exposes local HTTP endpoints for the dashboard while the process is running. It does not open a public listener.

## Static export boundary

`llmusage export html` writes a static snapshot directory. Share it only if you are comfortable sharing the aggregated usage values and labels captured in `snapshot.json`.
