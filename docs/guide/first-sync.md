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
llmusage sync --source kimi_code
llmusage sync --source pi
# gemini is no longer accepted as a source id; gemini-* model names are unchanged
```

The accepted source values match `cargo run -- --help`: `codex`, `claude`, `opencode`, `antigravity`, `kimi_code`, and `pi`. `gemini` is intentionally not accepted as a source id; `gemini-*` remains a model-name prefix only.

Kimi Code reads `~/.kimi-code/sessions/**/wire.jsonl` (or `KIMI_CODE_HOME/sessions`) and imports only explicit turn-scoped `usage.record` rows. It maps non-cached input, output, cache read, and cache creation independently, preserves raw models such as `kimi-code/k3`, and ignores aggregate, zero-token, non-turn, and malformed records.

Pi combines `~/.pi/agent/sessions` (or `PI_AGENT_DIR`) and `~/.omp/agent/sessions` under one `pi` source. Assistant usage rows preserve input, output, cache read/write, authoritative total, and diagnostic reasoning tokens. The local admission evidence includes real Oh My Pi samples and sanitized Pi-compatible fixtures; this machine had no Pi-only sample, so Pi-specific format changes remain an explicit evidence gap.

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

`--recent-days` enables recent-window signalling for callers. The current parser surface still scans existing cursors as needed to preserve correctness.

## Rebuild safely

```powershell
llmusage sync --rebuild
```

`--rebuild` resets parser-backed usage state source by source before reparsing local sources. Parserless Antigravity events, buckets, behavior facts, cursors, and source-file diagnostics are preserved. The rebuild is refused by default when file-backed imported history for a parser source depends on files that are now missing.

Token accounting is versioned per parser source. Databases containing rows
from the older accounting contract remain readable, but normal sync refuses to
mix old and corrected rows. Rebuild each affected source explicitly:

```powershell
llmusage sync --rebuild --source codex
llmusage sync --rebuild --source claude
llmusage sync --rebuild --source opencode
llmusage sync --rebuild --source kimi_code
llmusage sync --rebuild --source pi
```

The source marker advances only after the rebuild succeeds. `source-status` and
diagnostics expose `legacy_token_accounting`, `token_accounting_version`, and
an actionable warning while a source still needs rebuilding.

`llmusage serve` performs this repair automatically for safe legacy parser
sources before it binds the dashboard port. A source with lossy rebuild risk is
skipped with a warning: its historical reports remain readable, its normal
writes stay guarded, and the dashboard still starts. Parser, SQLite, or commit
failures for a source that passed the safety check stop dashboard startup.
Automatic repair never enables `--allow-lossy-rebuild`.

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
