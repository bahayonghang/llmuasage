# Runtime home layout

## Question

Which paths under the llmusage root are live data, regenerable cache, or one-shot leftovers?

## Product layout

`AppPaths::from_root` (`src/runtime/paths.rs:77-96`) owns:

| Path | Role | Cleanup note |
| --- | --- | --- |
| `llmusage.db` | Live usage database | Never delete from `clean`. |
| `llmusage.db-wal`, `llmusage.db-shm` | SQLite sidecars of the live DB | Not leftovers while the DB is open. |
| `bin/` | Hook wrapper directory. Created by bootstrap (`src/store/schema.rs:155-159`). | May be empty after hook removal. |
| `backups/` | Third-party config backups and migration DB copies. | Mixed. See below. |
| `exports/` | HTML export output (`src/commands/export.rs:22`). | Regenerable. Empty on this machine. |
| `logs/` | NDJSON shards. Caps: 10 MiB/shard, 30 MiB total, 7 files, 7 days (`src/runtime/logging.rs:23-26`). | Already self-pruning. |
| `cache/subscription-usage.json` | Subscription snapshot cache (`src/runtime/paths.rs:42-45`). | Regenerable, tiny. |
| `worker.lock` | Legacy lock path. | Not a data store. |
| `pricing/` | Content-addressed catalog files (`src/store/pricing_catalog.rs:651-681`, `726-739`). | New versions are added; old files are not deleted. Absent on this machine. |
| `pricing-cache-litellm.json`, `pricing-cache-models-dev.json` | 1-hour fetch cache at the root (`src/sync/pricing_refresh.rs:42-45`, `682-683`). | Overwritten in place. Absent on this machine. |
| `codex-tracer.db` | Separate Codex tracer DB (`src/commands/codex_tracer/mod.rs:34`). | Optional live DB, not a temp file. |

## Backup classes

Current code copies the live DB once and never overwrites:

- `backups/llmusage.db.pre-0.5.0` when schema is v0 (`src/store/schema.rs:163-165`, `412-414`).
- `backups/llmusage.db.pre-0.23-host` when schema is v22 (`src/store/schema.rs:166-168`, `416-418`).

`checkpoint_and_copy_db` (`src/store/schema.rs:420-431`) skips the copy when the destination already exists.

Integration cleanup writes timestamped `backups/*.bak` via `integrations::backup_file` (`src/integrations/mod.rs:138-151`). The spec forbids bulk-deleting those files, and says `llmusage.db.pre-0.5.0` stays available (`.trellis/spec/llmusage/backend/integration-file-contracts.md:53-55`). That rule is about hook cleanup and `uninstall`, not yet about a dedicated `clean` command.

These names are **not** produced by the current tree: `llmusage.db.pre-accounting-*`, `llmusage.db.pre-schema-*`. `pre-schema-v18-20260729-010307.sqlite` was a manual rollback kept by `.trellis/tasks/archive/2026-07/07-28-fix-serve-behavior-query-timeout/research/validation.md`.

`baselines/` is not an `AppPaths` directory. `.trellis/tasks/archive/2026-08/08-23-pi-omp-usage-accounting/research/export_baseline.py` created `~/.llmusage/baselines/08-23-pi-omp-usage-accounting/`.

## Sources

- `src/runtime/paths.rs`
- `src/runtime/logging.rs`
- `src/store/schema.rs`
- `src/store/pricing_catalog.rs`
- `src/sync/pricing_refresh.rs`
- `src/integrations/mod.rs`
- `.trellis/spec/llmusage/backend/integration-file-contracts.md`
