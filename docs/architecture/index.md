# Architecture

## Runtime layout

The runtime state lives under `~/.llmusage/`:

- `llmusage.db` stores cursors, events, buckets, project metadata, integration state, trigger state, and run logs.
- `bin/llmusage-hook.cmd` and `bin/llmusage-hook.sh` are the local wrappers called by external tools.
- `exports/` stores static HTML reports.
- `backups/` stores integration config backups used by uninstall.

## Data flow

1. A tool-specific hook or plugin triggers `llmusage hook-run`.
2. `hook-run` records the trigger signal and tries to acquire the global worker lock.
3. The worker runs the three local parsers in sequence.
4. New events are written into `usage_event`.
5. 30-minute UTC aggregates are upserted into `usage_bucket_30m`.
6. Query endpoints and local exports read the same SQLite database.

## Local-only guarantees

- No device token
- No account login
- No upload queue
- No remote API calls
- No GitHub public visibility probe

Project labels come from the local git remote when present. Only hashed local paths are stored.
