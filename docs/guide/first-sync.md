# First sync

`llmusage sync` imports local usage into SQLite. Report commands do not auto-sync, so run sync when the database is stale.

## Import all sources

```powershell
llmusage sync
```

Human progress is written to stderr. The final stdout summary is one table with a row per source and a `TOTAL` row; completed progress lines are cleared instead of being repeated above it. Redirected output contains no ANSI control sequences, and narrow terminals use compact headers without truncating numeric values.

If the embedded pricing catalog changed since the previous run, bootstrap reprices historical events before source scanning. Progress shows the old/new catalog versions, processed and total event counts, bucket reconciliation, and final elapsed time. This is a one-time upgrade for an unpinned embedded catalog; a current or pinned catalog skips the phase.

The summary includes `files`, `changed`, `skipped`, `seen`, `committed`, and `stored_events` per source. For file-backed sources, `skipped` means the stored cursor, size, mtime, head fingerprint, tail signature, and offset show the artifact is unchanged. For OpenCode, `skipped` means the SQLite high-water cursor found no newer rows. `committed` is the newly inserted event delta after SQLite dedupe; `stored_events` is the durable total currently in the database.

## Import one source

```powershell
llmusage sync --source codex
llmusage sync --source claude
llmusage sync --source opencode
llmusage sync --source antigravity
llmusage sync --source antigravity_ide
llmusage sync --source kimi_code
llmusage sync --source pi
llmusage sync --source omp
llmusage sync --source grok
# gemini is no longer accepted as a source id; gemini-* model names are unchanged
```

The accepted source values match `cargo run -- --help`: `codex`, `claude`, `opencode`, `antigravity`, `antigravity_ide`, `kimi_code`, `pi`, `omp`, and `grok`. `gemini` is intentionally not accepted as a source id; `gemini-*` remains a model-name prefix only.

Kimi Code reads `~/.kimi-code/sessions/**/wire.jsonl` (or `KIMI_CODE_HOME/sessions`) and imports only explicit turn-scoped `usage.record` rows. It maps non-cached input, output, cache read, and cache creation independently, preserves raw models such as `kimi-code/k3`, and ignores aggregate, zero-token, non-turn, and malformed records.

Pi reads `~/.pi/agent/sessions` (or `PI_AGENT_DIR`) as source `pi`. Oh My Pi reads `~/.omp/agent/sessions` as source `omp`. The two sources share one parse implementation. If a path overlaps, `pi` wins and `omp` skips only the conflicting files. Assistant usage rows preserve input, output, cache read/write, authoritative total, and diagnostic reasoning tokens. After upgrade, run one unbounded `llmusage sync` with no `--source` so legacy `pi` rows rebuild and `.omp` files land as `omp`. Later provider, project, cost, and behavior backfill uses `llmusage sync --rebuild --source omp`. The local admission evidence includes real Oh My Pi samples and sanitized Pi-compatible fixtures; this machine had no Pi-only sample, so Pi-specific format changes remain an explicit evidence gap.

Grok Build reads only direct sidecars under `~/.grok/sessions/*/*/` (or `GROK_HOME/sessions`). The primary path maps each `turn_completed` `params.update.usage` record to one precise event. Sessions without usage keep the older `_meta.totalTokens` plus `signals.json` total-only fallback. Cost remains `unpriced`. Any sidecar change replays the full session; a tracked missing sidecar preserves prior rows and blocks lossy rebuild until the file returns.

Other platforms can appear in `llmusage source-status` or the `dash` source picker as monitor-only candidates. They stay parserless until sanitized fixtures, token semantics, sync-twice tests, cursor/fingerprint regression tests, and privacy review exist.

Reasonix remains monitor-only: current session JSONL has no replayable per-turn usage fields, while older telemetry sidecars are mutable cumulative summaries. Importing those sidecars as events would create weak cursor semantics and double-counting risk, so they are not a fallback parser input.

## Emit NDJSON progress

```powershell
llmusage sync --json-events
```

This mode prints lifecycle and progress events as NDJSON on stdout. Pricing upgrades add `pricing_upgrade_started`, throttled `pricing_upgrade_progress`, `pricing_bucket_reconcile_started`, and `pricing_upgrade_finished` in that order. Use it for wrappers or UI adapters that need machine-readable progress.

Human progress does not depend on structured logging. For file diagnostics, use `LLMUSAGE_LOG=info` for pricing phase boundaries or `debug` for page progress; the default `warn` level records one liveness warning after 30 seconds.

## Recent-ready signal

```powershell
llmusage sync --recent-days 1
```

`--recent-days` imports only events whose UTC timestamp is inside the requested window (valid range: `1..=3650`). File-backed sources still inspect records when metadata alone cannot safely exclude them, while OpenCode pushes the cutoff into its SQLite query. Bounded imports never advance the full-history cursor, so a later ordinary `llmusage sync` can still recover older events. Use `--parallelism 1..32` to set the parser worker limit.

## Rebuild safely

```powershell
llmusage sync --rebuild
```

`--rebuild` reparses selected sources. Antigravity first stages native SQLite, then atomically replaces attributed parser history while retaining unattributed hook rows. Missing source files block rebuild by default; `--allow-lossy-rebuild` accepts missing-file loss, never unreadable or failed database parsing.

Token accounting is versioned per parser source. Databases containing rows
from an older accounting contract remain readable. Ordinary `llmusage sync`
preserves each selected legacy source's history, cursors, and accounting marker,
skips its imports, and warns that an explicit rebuild is required. When stdin, stdout, and stderr are all terminals, with no `--json-events`,
`--rebuild`, or `--recent-days`, you can explicitly accept loss for
`antigravity` or `antigravity_ide` to rebuild that product in the same run;
all other legacy sources remain skipped. Other current sources can still sync.
Restore any missing source files, then explicitly rebuild the affected source:

```powershell
llmusage sync --rebuild --source codex
llmusage sync --rebuild --source claude
llmusage sync --rebuild --source opencode
llmusage sync --rebuild --source antigravity
llmusage sync --rebuild --source antigravity_ide
llmusage sync --rebuild --source kimi_code
llmusage sync --rebuild --source pi
llmusage sync --rebuild --source grok
```

Antigravity's version-2 history requires repair before version-3 imports (via
the interactive terminal prompt or explicit rebuild). CLI and IDE rebuilds first
stage the complete native SQLite snapshot, then atomically replace attributed
parser rows and advance their accounting markers. A staging or transaction failure
preserves prior rows and markers. Unattributed hook-era rows remain in historical
totals under their original accounting, with a retained-history warning; they
are not converted to version 3.

`source-status` and diagnostics expose `legacy_token_accounting`,
`token_accounting_version`, and an actionable warning while a source still
needs rebuilding. After successful explicit repair, ordinary full or bounded
sync can import that source again.

`llmusage serve` detects legacy parser sources before binding the dashboard
port, preserves their history, and displays the accounting warning. It does
not rebuild them, including sources whose original inputs are missing or
unparseable. Historical reports remain available and the dashboard can start;
ordinary imports for those sources stay skipped until explicit repair.
`--allow-lossy-rebuild` is never enabled automatically.

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

Token quality labels are source descriptors, not runtime guesses: `precise` sources preserve input, output, cache read, cache creation/write, reasoning, and total channels; `total_only` sources do not claim subchannel precision; `estimated` sources are explicitly approximate; monitor-only or blocked sources are shown as unavailable/parserless instead of being imported.

For precise sources, `input_tokens` is non-cached input, cache channels are
reported separately, and parser-owned `total_tokens` is authoritative across
reports and dashboards. Reasoning remains a diagnostic subchannel and is not
added again when upstream output or total already includes it.
