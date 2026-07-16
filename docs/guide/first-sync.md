# First sync

`llmusage sync` imports local usage into SQLite. Report commands do not auto-sync, so run sync when the database is stale.

## Import all sources

```powershell
llmusage sync
```

Human progress is written to stderr. The final summary stays on stdout.

The summary includes `files`, `changed`, `skipped`, `seen`, `committed`, and `stored_events` per source. For file-backed sources, `skipped` means the stored cursor, size, mtime, head fingerprint, tail signature, and offset show the artifact is unchanged. For OpenCode, `skipped` means the SQLite high-water cursor found no newer rows. `committed` is the newly inserted event delta after SQLite dedupe; `stored_events` is the durable total currently in the database.

## Import one source

```powershell
llmusage sync --source codex
llmusage sync --source claude
llmusage sync --source opencode
llmusage sync --source antigravity
# gemini is no longer accepted as a source id; gemini-* model names are unchanged
```

The accepted source values match `cargo run -- --help`: `codex`, `claude`, `opencode`, and `antigravity`. `gemini` is intentionally not accepted as a source id; `gemini-*` remains a model-name prefix only.

Other platforms can appear in `llmusage source-status` or the `dash` source picker as monitor-only candidates. They stay parserless until sanitized fixtures, token semantics, sync-twice tests, cursor/fingerprint regression tests, and privacy review exist.

## Emit NDJSON progress

```powershell
llmusage sync --json-events
```

This mode prints lifecycle and progress events as NDJSON on stdout. Use it for wrappers or UI adapters that need machine-readable progress.

## Recent-ready signal

```powershell
llmusage sync --recent-days 1
```

`--recent-days` enables recent-window signalling for callers. The current parser surface still scans existing cursors as needed to preserve correctness.

## Rebuild safely

```powershell
llmusage sync --rebuild
```

`--rebuild` clears rebuildable usage rows, buckets, projects, and cursors before reparsing local sources. It is refused by default when file-backed imported history depends on source files that are now missing.

Token accounting is versioned per parser source. Databases containing rows
from the older accounting contract remain readable, but normal sync refuses to
mix old and corrected rows. Rebuild each affected source explicitly:

```powershell
llmusage sync --rebuild --source codex
llmusage sync --rebuild --source claude
llmusage sync --rebuild --source opencode
```

The source marker advances only after the rebuild succeeds. `source-status` and
diagnostics expose `legacy_token_accounting`, `token_accounting_version`, and
an actionable warning while a source still needs rebuilding.

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
