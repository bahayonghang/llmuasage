# CLI reference

This page describes the CLI for version `2.0.1`. Top-level help uses a compact table; root and nested help and argument diagnostics follow the saved interface language.

## Top-level help

```powershell
llmusage help
llmusage --help
llmusage -h
llmusage help --zh
```

`llmusage help`, `llmusage --help`, and `llmusage -h` follow the saved language. Use `llmusage help <COMMAND>` or `llmusage <COMMAND> --help` for command-specific help. `llmusage help --zh` forces Chinese for that invocation without saving a preference.

## Interface language

### `llmusage lang [en|zh|zh-CN]`

```powershell
llmusage lang
llmusage lang zh
llmusage lang en
llmusage --home .\work-profile lang zh-CN
llmusage --home .\work-profile daily --help
```

English is the default, regardless of system locale. `en` and `zh` are the saved language codes; `zh-CN` is an alias saved as `zh`. This setting covers CLI help, argument errors, human command/report/console text, supported log replay, and new `dash`/`tui` sessions. Commands, flags, enum values, reason codes, units, model names, and user data retain their original values.

Preferences live in `<runtime-root>/cli-settings.json`. Root precedence is explicit `--home` > `LLMUSAGE_HOME` > `~/.llmusage`, so each profile has its own language. `clean` retains its exception: explicit `--home` or the default root, ignoring `LLMUSAGE_HOME`. Reading preferences, help, version, argument errors, and non-interactive `lang` do not create the root, logs, or database. Missing settings use English; invalid or unreadable settings fall back to English with a repair diagnostic and are not rewritten on read.

`lang` needs no initialization, database, or worker lock. When stdin, stdout, and stderr are all terminals, it offers `English` / `中文（简体）`; empty input, `q`/`Q`, or EOF cancels without saving. Otherwise it prints the current language and explicit setting commands without reading stdin. Explicit `lang en|zh|zh-CN` saves the choice; a failed save reports an error and preserves the old complete file.

Each CLI invocation reads the setting once. A running terminal dashboard keeps its startup language; exit and reopen after a change. There is no live TUI language menu. Browser/desktop dashboards and exported HTML keep their independent language behavior. JSON/NDJSON/SSH/HTTP payloads and persisted diagnostics/file logs remain canonical; external errors and historical logs without supported presentation metadata may retain original detail. `--locale` is a separate report compatibility value, not an interface-language switch.

## Global options

```text
Usage: llmusage [OPTIONS] [COMMAND]
```

| Option | Meaning |
| --- | --- |
| `--home <PATH>` | Override `LLMUSAGE_HOME` and the default `~/.llmusage` runtime root |
| `--since <YYYY-MM-DD\|YYYYMMDD>` | Inclusive start date for report commands |
| `--until <YYYY-MM-DD\|YYYYMMDD>` | Inclusive end date for report commands |
| `--json` | Emit stable JSON for supported report commands |
| `--breakdown` | Include per-model breakdown rows or payloads where supported |
| `--order asc\|desc` | Sort report rows by period/activity |
| `--timezone UTC\|local\|+08:00` | Report timezone. `local` resolves the system IANA timezone and applies the offset for each date, including DST. If resolution fails, it uses the current fixed local offset. |
| `--locale <LOCALE>` | Report compatibility value: `en-US`, `zh-CN`, or `ja-JP`; does not select CLI/TUI language or introduce locale-specific number formatting |
| `--compact` | Use a narrower table layout |
| `--no-cost` | Hide cost columns and cost fields from report output |
| `--source codex\|claude\|opencode\|antigravity\|antigravity_ide\|kimi_code\|pi\|omp\|grok` | Restrict a top-level report or sync command to one source |
| `--host <LABEL>` | Restrict a report to one registered host label |
| `-A, --by-agent` | Add nested source rows to unified report JSON |
| `--sections daily\|weekly\|monthly\|session` | Add report periods to one combined output |
| `--all` | Show full daily history instead of the default last 7 days |
| `--instances` | Group daily rows by project/instance |
| `--project <PROJECT>` | Filter by project label, hash, or reference |

## Runtime logging

`llmusage` writes structured runtime diagnostics to local-only NDJSON shards at `~/.llmusage/logs/llmusage.ndjson.*` by default. Each line is one JSON object.

| Environment variable | Meaning |
| --- | --- |
| `LLMUSAGE_LOG=off\|error\|warn\|info\|debug\|trace` | Controls the local NDJSON log file; default is `warn` |
| `RUST_LOG=...` | Continues to control console stderr logging |

File logging does not write to report stdout and does not change `sync --json-events` stdout. Shards rotate at 10 MiB while the process is running and retain at most 30 MiB, seven files, and seven days. `logs` and `diagnostics` status include retained size/count plus dropped-event and maintenance-failure counters.

Human console messages and supported `logs` replay follow `lang`, using stable message metadata. Stored messages, fields, levels, filters, and historical records are not rewritten. Unknown or third-party details remain identifiable as original detail.

## Report commands

Report commands read the local database only.

### `llmusage` / `llmusage daily`

```powershell
llmusage
llmusage daily --all
llmusage daily --source codex --since 20260501 --until 20260518
llmusage daily --json --breakdown
```

Default command. Shows daily token and estimated-cost usage. Daily, weekly, and monthly text output uses aggregate and per-source rows with localized labels (`All` / `Agent` in English); CLI JSON is camelCase and adds nested source rows when `--by-agent` is set.

### `llmusage weekly`

```powershell
llmusage weekly
llmusage weekly --since 2026-05-04 --until 2026-05-10
llmusage weekly --by-agent --json
```

Groups usage by the Monday that starts each week.

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

### `llmusage <source> <period>`

```powershell
llmusage claude daily
llmusage codex monthly --json
llmusage opencode weekly --no-cost
llmusage antigravity session
llmusage antigravity-ide session
```

`claude`, `codex`, `opencode`, `antigravity`, and `antigravity-ide` each host `daily`, `weekly`, `monthly`, and `session`. A focused command injects the matching source filter, has the same data as `<period> --source <source>`, and removes the `Agent`/`Detected` comparison layer. Its JSON has no `agent` or `agents` fields. A duplicate matching `--source` is accepted; a conflicting value is rejected. `blocks` is deliberately not available in this tree.

This is a uniform llmusage extension, not a source-by-source reproduction of ccusage's capability matrix.

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

Prints one statusline-friendly usage summary. Its labels follow `lang`; model names, units, and numerical values retain their semantics. Caches are separate at `statusline-cache/latest.en.txt` and `latest.zh.txt` under the runtime root; another language's cache and legacy `latest.txt` are never used as a fallback.

## Setup and sync commands

### `llmusage init`

```powershell
llmusage init
```

Creates the local runtime and bootstraps the usage database. It does not modify third-party tool configuration.

### `llmusage sync`

```powershell
llmusage sync
llmusage sync --source codex
llmusage sync --source kimi_code
llmusage sync --source pi
llmusage sync --source grok
llmusage sync --recent-days 1
llmusage sync --recent-days 30 --parallelism 4
llmusage sync --json-events
llmusage sync --verbose
llmusage sync --rebuild
llmusage sync --rebuild --allow-lossy-rebuild
```

`--source`, `--recent-days`, and `--parallelism` use the same validation contract as `POST /api/jobs` and the public `JobRegistry` API. Invalid values fail with stable codes: `unknown_source`, `invalid_recent_days`, or `invalid_parallelism`.

Imports local sources, then pulls registered SSH remotes. One unreachable remote is skipped with a warning; the process exit code stays success if local sync succeeded. Uncontacted remotes are not swept to `missing` and do not block `--rebuild`. `--json-events` also emits `remote_host_started`, `remote_host_finished`, and `remote_host_skipped`.

Before source scanning, bootstrap may upgrade an unpinned embedded pricing catalog and reprice historical events. Ordinary sync detects selected legacy token-accounting sources, keeps that source's existing data, skips that source's writes for the round, and warns that repair is `llmusage sync --rebuild --source <source>`. When stdin, stdout, and stderr are all terminals, with no `--json-events`, `--rebuild`, or `--recent-days`, users can explicitly accept loss for `antigravity` or `antigravity_ide` to rebuild that product in the same run; all other legacy sources remain skipped. Claude Code, Codex, Grok Build, Kimi Code, and Oh My Pi (OMP) must use this ordinary-sync vs explicit-rebuild split.

Human stderr reports catalog versions, processed/total events, bucket reconciliation, explicit-repair warnings for skipped legacy sources, remote-host skip warnings, and elapsed completion time. `--json-events` writes the same lifecycle to NDJSON-only stdout, including remote-host events and the existing pricing events. Ordinary sync does not emit `token_accounting_repair_finished` as success. A current catalog or pinned snapshot/overlay emits no pricing events. `--allow-lossy-rebuild` on the CLI requires explicit `--rebuild` and is ignored by ordinary sync (use the interactive terminal prompt to confirm Antigravity lossy rebuild during ordinary sync).

Set `LLMUSAGE_LOG=info` for structured pricing start/reconcile/finish file records, or `debug` for throttled page progress. The default `warn` file level records one liveness warning if repricing continues beyond 30 seconds; terminal progress remains visible at every file-log level.

By default the human stdout summary shows the last seven local days (today included) from the database, using the same query, window, and timezone as `llmusage daily`, so its `TOTAL` equals `llmusage daily --json` totals at that moment. Columns are `Date`, `Source`, `Input`, `Output`, `Cache Create`, `Cache Read`, `Total Tokens`, and `Cost (USD)`. Dates run newest first with the date only on a day's first row, sources are sorted by cost, a `(total)` row follows any day with more than one source, dividers separate days, and `TOTAL` covers the window. `--source` limits it to one source. A trailing `*` marks rows with unpriced events (cost is a lower bound). An empty week prints `No usage in the last 7 days.`; other ranges use `llmusage daily --since/--until`. A final line digests this run (files, events seen, events written including re-imported ones, bytes, parse/write time); undetected sources share one `Not detected:` line. If the usage query fails, sync still succeeds and prints `Usage summary unavailable` on stderr. Startup stderr folds schema migrations into one `✓ Database schema upgraded to vN (…)` line and hides lock acquisition under one second. On a narrow terminal the cache columns are hidden before any number would be cut. Repair hints print commands without quotes or trailing punctuation so they can be copied as-is.

With `--verbose` (not combinable with `--json-events`), the summary is instead one closed grid with ten columns: `SOURCE`, `FILES`, `CHANGED`, `SKIPPED`, `SEEN`, `COMMITTED`, `STORED`, `BYTES`, `PARSE`, and `WRITE`. It has an outer frame, vertical cell borders, a header separator, one row per source, and a separator before `TOTAL`; labels follow `lang`. Completed progress remains on stderr and is not repeated as permanent success lines. Diagnostics appear after the bottom border, grouped by source with faults separated from record/accounting information. Protected or blocked sources show `COMMITTED = 0`, retained `STORED`, `CHANGED = 0`, `BYTES = 0`, `WRITE = 0`, and `FILES` showing the discovered file count from read-only coverage. `skipped` is derived from existing cursor/fingerprint evidence for file-backed sources and from the OpenCode SQLite high-water cursor for DB-backed sync. `committed` is the newly inserted event delta after SQLite dedupe. Redirected output has no ANSI escapes, and narrow terminals use compact headers without truncating numeric values; full numbers can still exceed a very narrow terminal. See the [captured fixture example](../guide/first-sync#import-all-sources).

Record diagnostics below the table distinguish malformed records, oversized records, intentional skips, and accounting anomalies. Each source retains at most eight record samples. The summary states the total count and the number of samples omitted. A sample can show a stable reason, its location value, and a safe file basename together. JSONL samples use `@` for a nonzero byte offset. ZCode samples use `timestamp_ms=` for the existing millisecond timestamp locator. Full private paths, path hashes, and record bodies are not printed. The diagnostic `skipped` count refers to records; the table's `SKIPPED` column refers to files.

Codex tool-output records above the 4 MiB limit can be skipped with `oversized_non_usage_record`; the bounded reader continues to later usage records. Grok records marked `usageIsIncomplete` retain their reported token values and an accounting warning. The parser does not replace those values with fallback estimates. These informational diagnostics do not mean that the entire source failed.

Source failures, such as a missing tracked Antigravity database, are separate from malformed record counts. The stored source status retains the failure after the command exits. A blocked source keeps its imported history and cannot be certified as a clean accounting repair. File inventory reflects the most recent completed observation; a stored `live` state does not prove that a path exists now.

Record diagnostic totals count notices, which need not represent distinct records or failed sources. Recovery prompts default to keeping history: `k`/`K`, empty input, EOF, read failure, or two invalid attempts keep it; `r`/`R` explicitly accepts loss only within the terminal/flag gate above. Prompts distinguish preserved events, current/new inputs, physical absence, still-present out-of-scope inputs, and unreadable/incomplete discovery. Coverage counts are not an exact event-loss estimate. Access/discovery failures must be repaired before retrying.

`WRITE` reports recorded writer elapsed time. Ordinary shard timing includes provider mapping, transaction start, resets, persisted facts and cursors, and commit; host-prefix preparation and in-memory behavior deduplication occur before that timer. Antigravity records each product's apply and marker time separately, excluding the shared group reset, transaction start, and commit. Do not add those product values to estimate the entire transaction. `PARSE` is the remaining source-stage time after recorded `WRITE`, so it is not a measure of parser CPU time. `BYTES` follows each reader's input accounting, which can include inventoried file sizes; it is not a physical disk-I/O measurement.

Antigravity checks coverage of tracked inputs after discovering and fingerprinting the CLI and IDE roots. When all selected products are blocked, sync returns before usage decoding. The check still performs filesystem work. A run with only some products blocked keeps cross-root decoding to resolve copied databases and native product ownership.

Coverage diagnostics distinguish physically missing paths, existing tracked paths outside current discovery (including legacy JSON inputs or a changed root), and access or discovery failures. Restore the original supported input or correct the root/access configuration before retrying. `--allow-lossy-rebuild` accepts unrebuildable history loss only with an explicit rebuild; access and discovery failures still block replacement. Failed coverage checks do not advance usage cursors or the last successful inventory observation.

### `llmusage remote`

```powershell
llmusage remote add <label> <ssh-target> [--command <path>]
llmusage remote list
llmusage remote remove <label>
llmusage remote sync
llmusage remote sync --host <label>
```

Registers, lists, removes, or imports SSH remote hosts. `add` probes `ssh <target> <command> --version` and `remote handshake` before it writes a host row. `remote sync` imports shards only; it does not run local parsers. `llmusage sync` already includes every registered SSH host after the local driver.

## Status and diagnostics

### `llmusage status`

```powershell
llmusage status
```

Prints a human-readable database, source, and recent-run summary.

### `llmusage source-status`

```powershell
llmusage source-status
```

Prints parser-backed source and monitor-only platform status, grouped by host. Each host has one read-only lifecycle state: `never_contacted`, `unreachable`, or `idle`. `live` is a sync-event signal only.

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

Runs health checks. `--refresh-pricing <PATH>` validates a complete internal-v1, catalog-v2, or native LiteLLM base snapshot, stores a content-addressed copy under `~/.llmusage/pricing/`, clears any active overlay, and recomputes per-event costs. URLs are refused. This option replaces the complete base; it is not an incremental overlay.

### `llmusage catalog`

```powershell
llmusage catalog apply .\pricing-overlay.json
llmusage catalog status
llmusage catalog status --json
llmusage catalog reset
```

`catalog apply` validates and activates a local v2 overlay. The overlay is always merged with its recorded base, so applying a second overlay does not stack it on the previous effective catalog. Models with an existing `id` are replaced as complete definitions; `remove_models` fails when an id is unknown. Activation stores content-addressed base/overlay/effective files and recomputes persisted event and bucket costs before switching catalog metadata.

`catalog status` distinguishes the base, optional overlay, and effective catalog. JSON output includes each layer's declared version, runtime identity, schema version, file, model count, expanded source-rule count, and `rebase_available`.

`catalog reset` removes the overlay and restores its recorded base. A snapshot base remains pinned; an embedded base returns to the current embedded catalog. With no overlay, reset is idempotent.

Minimal overlay:

```json
{
  "schema_version": 2,
  "kind": "overlay",
  "version": "team-pricing-2026-07",
  "models": [
    {
      "id": "team-model",
      "sources": ["codex", "opencode"],
      "matches": [
        { "value": "team-model", "mode": "exact" }
      ],
      "rates": {
        "default": {
          "input_per_mtok": 1.0,
          "cached_per_mtok": 0.1,
          "cache_creation_per_mtok": 1.25,
          "output_per_mtok": 6.0
        },
        "tiers": [
          {
            "name": "long_context",
            "prompt_tokens_above": 272000,
            "input_per_mtok": 2.0,
            "cached_per_mtok": 0.2,
            "cache_creation_per_mtok": 2.5,
            "output_per_mtok": 9.0
          }
        ]
      },
      "context_window": 1050000
    }
  ],
  "remove_models": []
}
```

`exact` matches only the normalized complete model id. `family` also accepts dash/dot-normalized family suffixes. Exact matches win over family matches, then the longest matcher wins. `version` is an audit label and never controls a file path. Tier thresholds are selected per `usage_event` from input + cache-read + cache-creation tokens; bucket totals never trigger a tier again.

### `llmusage logs`

```powershell
llmusage logs
llmusage logs --limit 50 --level warn
llmusage logs --command sync --json
```

Queries local structured runtime log entries and recent SQLite `run_log` command records. Filters are applied to the local runtime log file and the `run_log` command label; no usage raw JSON, prompts, or responses are dumped.

## Local UI commands

### `llmusage dash`

```powershell
llmusage dash
```

Interactive terminal dashboard. The old hidden `tui` command is a deprecated alias. Both use the current runtime root's saved language once at startup, including all nine panels, navigation, help/settings, source picker, sync overlay, quota, and loading/empty/error states. Exit and reopen to apply a changed `lang`; there is no runtime language switch. Browser and desktop language remain independent.

Controls: `tab`/`shift-tab` or `1`-`9` switch views; `j`/`k`, arrows, Page Up/Page Down, Home/End, or the mouse wheel select rows; `o` cycles sortable columns and `O` reverses direction; `h`/`l` change the active time window where applicable; `s` opens the source picker; `r` refreshes dashboard data; `R` toggles data auto-refresh; `m` toggles hidden Usage emails; `y` opens Source Sync / Platform Monitor; `x` runs sync through the existing sync worker lock for the current source filter; `?` opens help/settings; and `q` exits. Daily/Monthly Enter opens detail and Esc returns; Stats Enter selects a day, graph clicks open its breakdown, and Esc closes that selection.

Usage fetches quota on entry, using a five-minute cache when fresh; `r` bypasses that cache. `R` does not poll quota APIs. Existing CLI credentials are read-only, emails are hidden by default, and provider/model/account values remain unchanged. The `y` overlay shows source sync and monitored platforms; `x` does not automatically accept a lossy rebuild. Language changes do not alter shortcuts, sorting, selection, mouse navigation, themes, or NoColor behavior.

### `llmusage serve`

```powershell
llmusage serve
llmusage serve --port 37421
llmusage serve --public --no-open --port 37421
```

Starts the full web dashboard and local JSON API on `127.0.0.1` by default. Startup keeps existing usage and shows the token-accounting warning; it does not implicit-rebuild, and an unparseable legacy source does not stop the dashboard. `--public` binds `0.0.0.0` but exposes only the read-only aggregate dashboard allowlist (`/`, assets, `/api/dashboard`, and `/api/health`); projects, logs, diagnostics, jobs, behavior/Explorer detail, and writes remain loopback-only. The public aggregate view has no authentication or TLS. `--no-open` suppresses browser launching, and SSH sessions skip the automatic browser launch automatically. Use loopback through an SSH tunnel when remote access needs the full local API.

SSH is also a data channel: `llmusage remote add` / `llmusage sync` pull normalized shards from a user-owned host. That path is separate from dashboard tunneling.

### `llmusage codex-tracer`

```powershell
llmusage codex-tracer
llmusage codex-tracer --port 9876
llmusage codex-tracer --no-open
llmusage codex-tracer --rebuild
```

Starts the Codex-only local dashboard. It reads rollout JSONL from `$CODEX_HOME/rollout/` or `~/.codex/rollout/`, writes a separate `codex-tracer.db`, and serves a dedicated browser UI with detailed token accounting and thread tracking.

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

Cleans up hooks, plugins, wrapper scripts, and atomic-write residue left by older llmusage releases. Only llmusage-owned entries are removed; sibling user configuration and historical backups are preserved. A no-op cleanup does not create backups or integration audit rows. Machines upgraded from a hook-enabled release should run this command once. `--purge` also removes the runtime root and its usage database.
