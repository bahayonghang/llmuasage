# Local runtime home inventory

## Question

What is actually on this machine under the default runtime root, and which of it is large?

## Scope

- Resolved home: `C:\Users\lyh\.llmusage` (`%USERPROFILE%\.llmusage`).
- `LLMUSAGE_HOME` was unset.
- Checked and absent: `%USERPROFILE%\AppData\Roaming\llmusage`, `%USERPROFILE%\AppData\Local\llmusage`, `D:\Documents\Code\CLI\llmusage\.llmusage`.
- Inventory is names, byte sizes, and mtimes only. File contents were not read.

## Top level

| Entry | Kind | Bytes | Files | Last write | Role |
| --- | --- | ---: | ---: | --- | --- |
| `llmusage.db` | file | 1,596,936,192 | 1 | 2026-10-07 20:27 | Live DB |
| `llmusage.db-shm` | file | 32,768 | 1 | 2026-10-08 06:47 | Live SQLite sidecar |
| `llmusage.db-wal` | file | 0 | 1 | 2026-10-07 20:33 | Live SQLite sidecar |
| `backups/` | dir | 4,846,220,884 | 27 | 2026-08-20 20:14 | Config backups plus old DB copies |
| `baselines/` | dir | 1,160,123,446 | 3 | 2026-08-22 21:38 | Research dump, not product layout |
| `codex-tracer.db` | file | 57,344 | 1 | 2026-06-16 01:25 | Optional tracer DB |
| `cache/subscription-usage.json` | file | 1,037 | 1 | 2026-08-19 02:33 | Subscription cache |
| `logs/` | dir | 0 | 7 | 2026-10-08 02:42 | Empty NDJSON shards from today |
| `bin/` | dir | 0 | 0 | 2026-07-28 01:56 | Empty wrapper dir |
| `exports/` | dir | 0 | 0 | 2026-04-21 20:46 | Empty export dir |
| `worker.lock` | file | 0 | 1 | 2026-04-21 21:09 | Legacy lock |
| `pricing/` | absent | 0 | 0 | — | No catalog files |
| pricing cache JSON at root | absent | 0 | 0 | — | No fetch cache |

Approximate total: 7.60 GB. The live database is 1.60 GB. The other ~6.01 GB is old database copies.

## `backups/` database copies

| File | Bytes | Last write | Produced by current code |
| --- | ---: | --- | --- |
| `llmusage.db.pre-0.23-host` | 1,160,073,216 | 2026-08-20 20:14 | Yes, once, from schema v22 |
| `llmusage.db.pre-0.5.0` | 205,832,192 | 2026-05-06 22:41 | Yes, once, from schema v0 |
| `llmusage.db.pre-accounting-v2-lossy-20260716-144952.sqlite` | 1,160,073,216 | 2026-07-16 01:49 | No |
| `llmusage.db.pre-accounting-v3-codex-lossy-20260722-142912.sqlite` | 1,160,073,216 | 2026-07-22 01:29 | No |
| `llmusage.db.pre-schema-v18-20260729-010307.sqlite` | 1,160,073,216 | 2026-07-28 12:03 | No; manual v17 rollback from the 2026-07-28 task |
| `llmusage.db.pre-schema-v18-20260729-010307.sqlite-shm` | 32,768 | 2026-07-28 21:31 | Sidecar of the manual backup |
| `llmusage.db.pre-schema-v18-20260729-010307.sqlite-wal` | 0 | 2026-07-28 21:31 | Sidecar of the manual backup |

The other 20 backup files are integration `*.bak` files plus `codex_notify_original.json`. Together they are about 63 KB (4,846,220,884 minus the rows above).

## `baselines/`

| File | Bytes |
| --- | ---: |
| `08-23-pi-omp-usage-accounting/llmusage.db.pre-omp-split` | 1,160,073,216 |
| `08-23-pi-omp-usage-accounting/baseline_pi_identity.csv` | 49,895 |
| `08-23-pi-omp-usage-accounting/aggregate_baseline.json` | 335 |

Created by the archived research script `export_baseline.py`, not by the CLI.

## Logs

Seven `llmusage.ndjson.2026-10-08.*.000` shards, all 0 bytes, mtimes 2026-10-08 00:10 through 02:42. They are inside the current 7-file retention cap. They are not the disk problem.

## Implication

A useful `clean` overview has to separate live data, integration config backups, code-owned migration copies, and unrecognized snapshots. Deleting logs, cache, or `*.bak` would not reclaim meaningful space on this machine.
