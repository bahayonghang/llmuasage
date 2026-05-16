# Architecture

## Runtime layout

The runtime state lives under `~/.llmusage/`:

- `llmusage.db` stores schema metadata, cursors, events, buckets, project metadata, source-file diagnostics, integration state, trigger state, pricing metadata, worker lock metadata, and run logs.
- `bin/llmusage-hook.cmd` and `bin/llmusage-hook.sh` are the local wrappers called by external tools.
- `exports/` stores static HTML reports.
- `backups/` stores integration config backups used by uninstall.

## Data flow

1. A tool-specific hook or plugin triggers `llmusage hook-run`.
2. `hook-run` records the trigger signal and tries to acquire the global worker lock.
3. The worker runs the registered local parsers in sequence: Codex, Claude, OpenCode, and Gemini.
4. Each parser emits `SyncShard` batches; the writer resets replaced file rows, writes events, updates cursors, and stamps source-file state in one commit protocol.
5. New events are written into `usage_event` with persisted cache-aware cost/pricing metadata; optional raw archive rows stay in `usage_event_raw`.
6. 30-minute UTC aggregates, including cost/pricing rollups, are upserted into `usage_bucket_30m`.
7. Report commands, query endpoints, TUI, and local exports read the same SQLite database.

## Local-only guarantees

- No device token
- No account login
- No upload queue
- No remote API calls
- No GitHub public visibility probe

Project labels come from the local git remote when present. Only hashed local paths are stored. Pricing refreshes use a user-provided local JSON file; llmusage does not fetch pricing data from the network.

## Report layer

`daily`, `monthly`, `session`, `blocks`, and `statusline` are read-only SQLite views. They reuse `usage_event` as the report source of truth and keep costs labeled as `estimated_cost_usd`; since 0.5.1 that value is read from persisted `cost_with_cache_usd` rather than recomputed at query time. Daily human rendering now defaults to one aggregate ccusage-style table with `Input / Output / Cache Create / Cache Read / Total Tokens / Cost (USD)`; source separation remains available through filters and breakdown rows, while JSON payloads stay aggregate and snake_case. Session reports use `session_id` metadata when available and fall back to stable source-file keys for older databases. `statusline` may write a tiny local cache under `~/.llmusage/statusline-cache/`; it does not upload or call network APIs.


## 0.5.x integration surface

The ccr-ui adapter surface is intentionally thin: `Dashboard::overview`, `trends_daily`, `home_overview`, `heatmap`, `logs`, `diagnostics`, and `JobRegistry` all read or mutate the same local SQLite state. JSON fields are snake_case across CLI reports, HTTP API responses, and static export snapshots. Schema migrations are explicit and versioned through v10; v10 records `pricing_catalog_version`, while 0.5.1 persists the active local snapshot under `~/.llmusage/pricing/<catalog-version>.json` so later sync writes stay on the same local catalog.

`worker_lock` serializes CLI, hook, and library workers. CLI/library sync waits through `Store::acquire_worker_lock_with`, while high-frequency hook-run keeps the legacy non-blocking path and skips if another worker is active.
