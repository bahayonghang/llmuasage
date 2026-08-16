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
| `~/.llmusage/backups/` | Historical integration backups plus database/pricing recovery material |
| `~/.llmusage/exports/` | Static HTML exports |
| `~/.llmusage/logs/llmusage.ndjson.*` | Local structured runtime diagnostics and command tracing |
| `~/.llmusage/pricing/` | Content-addressed local base, overlay, and effective pricing catalogs |

Runtime root precedence: `--home <PATH>` > `LLMUSAGE_HOME` > `~/.llmusage`.

Current releases do not install hooks or plugins. After upgrading a machine that used a hook-enabled release, run `llmusage uninstall` once to remove only llmusage-owned legacy configuration entries, wrappers, and atomic-write residue. Historical `*.bak` files and usage data remain; `uninstall --purge` is the explicit command that removes the runtime root.

## What is not uploaded

`llmusage` does not create an account session, device token, upload queue, or remote usage API call. Report, dashboard, and export surfaces read local SQLite.

Project labels are derived locally. Sensitive path dimensions are stored as hashes where the schema needs stable grouping.

Runtime diagnostics stay local. `LLMUSAGE_LOG` controls the NDJSON log files (`off`, `error`, `warn`, `info`, `debug`, or `trace`; default `warn`), while `RUST_LOG` controls console stderr. Files rotate at 10 MiB during a running process and retain at most 30 MiB, seven files, and seven days. The local `logs`/`diagnostics` status reports retained bytes and files, queue-dropped events, and rotation/retention failures. Runtime log events include command labels, run ids, sources, module targets, and error summaries; they do not intentionally record prompts, responses, or raw source JSON. Paths may appear in human error summaries, so treat diagnostics bundles as local troubleshooting artifacts.

Use `llmusage logs --limit 50 --level warn` to query recent runtime log entries across retained shards and SQLite `run_log` records. The command reads only local files/database rows and does not upload anything. Rotation and retention run continuously while the process writes logs; a restart is not required.

## Normal sync is retention-safe

```powershell
llmusage sync
```

Normal sync imports new/changed local source artifacts. If a file-backed source that was previously imported is now missing, sync keeps imported usage history and marks the source file as missing for diagnostics.

## Normal sync repairs safe legacy accounting

An unbounded normal `llmusage sync` detects selected parser sources that still
use an older token-accounting contract. It warns before changing data, checks
every automatic target for missing inputs and protected history, then resets
only the legacy subset and parses each selected source once.

If any target is lossy, no automatic target is reset. Restore the source files
and rerun normal sync, or use explicit rebuild flags only when you intentionally
accept the documented deletion. `sync --recent-days N` never auto-repairs
legacy accounting because a full reset followed by a bounded import would
discard history outside the window.

## Rebuild can be destructive

```powershell
llmusage sync --rebuild
```

`--rebuild` resets parser-backed usage state source by source before reparsing local sources. A rebuild that would delete unattributed hook-era Antigravity rows is refused even with `--allow-lossy-rebuild`. If imported file-backed history for a parser source depends on files that are now missing, llmusage refuses the rebuild before any reset.

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

Neither normal-sync nor startup automatic repair enables
`--allow-lossy-rebuild`. Parserless sources are not migration targets.

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

`llmusage serve` binds to `127.0.0.1` by default. Its loopback router contains the full local dashboard, including projects, logs, diagnostics, cursor health, job reads, behavior analytics, Cost Explorer, and guarded write routes.

`llmusage serve --public` explicitly binds `0.0.0.0` and selects a separate read-only router. Only the browser shell/assets, a field-allowlisted aggregate `/api/dashboard` projection, and a fixed minimal `/api/health` response are mounted. Raw logs, diagnostics, local path/project fields, internal errors, job state, and all mutation routes are absent rather than protected by `Host` or `Origin` headers. Remote diagnostics would require a future explicit opt-in with authentication.

The reduced public view still has no authentication or TLS and reveals aggregate usage, model, and source data. Do not expose it directly to an untrusted network; use a firewall or authenticated reverse proxy. Prefer the loopback listener through an SSH tunnel when full dashboard capabilities are required remotely.

## Static export boundary

`llmusage export html` writes a static snapshot directory. Share it only if you are comfortable sharing the aggregated usage values and labels captured in `snapshot.json`.
