# Design: tokscale-Informed Collection and TUI Migration

## Design Principles

1. Keep `llmusage`'s source/write/query contracts intact. `tokscale` is a reference for catalog, scanner, cache, and TUI behavior, not a replacement architecture.
2. Separate monitoring from importing. Platform discovery can be broad; writing token usage must remain evidence-gated.
3. Make trust visible. Users should see whether a platform is precise, total-only, estimated, parserless, blocked by missing samples, or unavailable.
4. Keep the TUI thin. The TUI should render dashboard/query/job state and trigger existing sync flows; it should not own parser or scanner logic.

## Current Boundaries To Preserve

- `SourceKind` is the stable persisted source id.
- `SourceDescriptor` describes supported source activation, capabilities, usage quality, and privacy.
- `registered_parsers()` is the executable parser fan-out.
- `SourceParser::parse()` emits `SourceSyncStats` and optional `SyncEvent` progress while writing through `SyncRunWriter`.
- `SyncShard`/`SyncRunWriter::commit_shard()` is the durable write protocol.
- `source_file` and `source_sync_status` track file state and sync outcome.
- `Dashboard` and web/job APIs are the shared reporting and progress contracts.

## Architecture

### 1. Platform Catalog and Monitoring Registry

Add a platform-monitoring layer beside, not inside, `SourceKind`:

- `SourceDescriptor`: existing parsed/importable sources.
- `PlatformMonitorDescriptor`: all monitored platforms, including parserless candidates.

Suggested descriptor fields:

- `platform_id`: stable display/status id such as `gemini`, `cursor`, or `zed`.
- `display_name`.
- `source_kind: Option<SourceKind>` for platforms that already persist usage.
- `roots`: env var, home-relative, XDG data/config, or explicit settings roots.
- `patterns`: file/database patterns from tokscale evidence.
- `parser_status`: `registered`, `planned`, `blocked_no_samples`, `unsupported`, or `external_only`.
- `usage_quality`: precise, total-only, estimated, or unavailable.
- `privacy`: local artifacts, local database, auth cache, or remote-only.
- `notes` / `next_action`: short explanation surfaced in CLI/TUI.

This avoids adding persisted source ids before a parser is trustworthy. Existing sources map to both a `SourceDescriptor` and a monitor descriptor; candidate platforms only have monitor descriptors.

Initial candidate matrix should be seeded from tokscale's `ClientDef` list and scanner-specific discoveries:

- Existing / near-existing: Codex, Claude, OpenCode, Antigravity.
- High-value missing: Gemini, Cursor, Copilot, Zed, Kiro, Goose, Grok, Kimi/Qwen.
- OpenCode/Claude-adjacent variants: Roo Code, Kilo Code, Kilo CLI, Cline, Codebuff, Crush.
- Additional monitored candidates: Warp/Oz, Amp, Hermes, Trae, OpenClaw, Pi/Droid, Gajae-Code, Synthetic.

Monitoring output should be available through CLI status/doctor paths and through the TUI source/status surfaces.

### 2. Scanner and Inventory

Introduce a source/platform inventory service that can be reused by parsers, source-status, sync progress, and TUI:

```text
MonitorDescriptor + Settings + Env/Home
  -> InventoryCandidate roots
  -> ProbeResult { available, files/dbs, skipped, warnings, parser_status }
```

Implementation guidance:

- Reuse tokscale's good ideas: env-aware roots, extra scan paths, per-platform filters, dedupe by canonical path, channel DB discovery, and sidecar filtering.
- Keep `llmusage` parser ownership of source-file inventory so ADR 0006 stale-live sweeps remain correct.
- Add inventory helpers first for existing sources, then extend to monitor-only platforms.
- Add settings support only where it solves a discovered path gap, such as explicit OpenCode DB paths or extra roots.

### 3. Cache and Fingerprints

Use tokscale's cache design as a source of invariants, but integrate with `llmusage` storage:

- Prefer SQLite-backed metadata on `source_file` or a small cache table over a separate bincode cache file.
- Track file size, mtime, lightweight sample hash, optional full hash, parser version, and token-semantics version.
- For SQLite sources, include database size/mtime plus WAL/SHM sidecar evidence when relevant.
- For append-only JSONL sources, support incremental offsets only when prefix hash and newline boundary prove the previous parse prefix is unchanged.
- Invalidate cache when parser version, schema version, fingerprint, or token semantics change.
- Preserve lossy rebuild safeguards and never treat unreadable inventory as a signal to mark files missing.

The first implementation should optimize existing Codex/Claude/OpenCode. Parser candidates should inherit the same cache contract only after fixture coverage exists.

### 4. Trust and Token Semantics

Every parser promotion must include:

- Sanitized fixture(s) or generated fixtures that preserve real schema shape.
- Explicit mapping for input, output, cache read, cache creation/write, reasoning, total, model, timestamp, project/session, and source file id.
- Behavior for missing token fields and cumulative-vs-delta counters.
- Sync-twice test proving unchanged artifacts do not duplicate usage.
- Cursor/cache regression test proving append-only and rewritten-file behavior.
- Source-status test showing parser availability and quality labels.

If a source only exposes totals, import it as `total_only` and keep subchannels zero/unknown by design. If values are estimated, label them as estimated in status and reports.

### 5. Sync and Monitoring Events

Extend sync progress without changing the write boundary:

```text
ProbeStarted / ProbeFinished
SourceStarted / Progress / SourceFinished
PlatformBlocked / PlatformUnavailable / CacheHitSummary
Finished
```

If adding enum variants is too disruptive, use an internal status model first and adapt it to existing `SyncEvent::Progress` fields. The acceptance goal is user-visible differentiation between:

- scanned but unchanged,
- parsed and committed,
- parserless but detected,
- blocked because samples are missing,
- unavailable because no local artifacts were found,
- error because inventory or parse failed.

### 6. TUI Migration

Refactor `src/tui` toward the tokscale app shape:

```text
TuiConfig
TuiApp
  state: active tab, source filter, sort, scroll, theme, job status
  data_loader: Dashboard + source monitor + JobRegistry client
  dialogs: source picker, settings/theme, export/help
EventHandler
  tick, key, mouse, resize
Renderer
  header, tab content, footer/status, dialogs
```

Target tabs:

- `Overview`: totals, recent trend, top models/sources, health hints.
- `Usage` or `Sync`: current sync job, refresh button, platform/source status, last run summary.
- `Models`: sortable model/provider/source table.
- `Daily`: tokscale-style daily table with detail drilldown.
- `Hourly`: hourly table/profile view; minutely can be hidden behind data availability or later follow-up.
- `Stats`: contribution/calendar-style stats, streaks, source breakdown.
- `Agents` / `Behavior`: adapt `llmusage` behavior/tool/project data into a tokscale-like agent/activity view.
- `Sources` / `Health`: either a tab or dialog if it fits better after layout tests.

TUI rendering rules:

- Use `ratatui::backend::TestBackend` tests for normal and narrow widths.
- Keep responsive short labels, stable click areas, footer help that fits, and no text overlap.
- Use source/theme registries instead of scattered hard-coded colors.
- Use existing dashboard queries and job APIs; no parser/scanner logic in rendering code.

### 7. Compatibility and Migration

- Existing stored usage rows, source ids, CLI output, and web dashboard endpoints must continue to work.
- Any DB migration must be additive and reversible by ignoring new columns/tables.
- Candidate monitor ids are not persisted as `SourceKind` until parser promotion.
- If cache metadata proves risky, it can be disabled per source without disabling sync.
- The old TUI panel data can be retained behind the new data loader until each migrated tab has test coverage.

## Trade-offs

- Broad platform monitoring vs broad parsing: monitoring gives useful visibility with low trust risk; parsing without samples risks wrong cost/token data.
- SQLite cache vs bincode cache: SQLite fits `llmusage` durability and diagnostics better; bincode is simpler but creates another state file and locking story.
- SIMD JSON vs current serde path: SIMD may help large JSONL but should follow measurement because it adds dependency and unsafe/input-buffer considerations.
- Full tokscale TUI clone vs llmusage-native dashboard: visual and interaction parity is the goal, but data contracts should stay llmusage-native.

## Rollback

- Disable new cache metadata reads behind a feature/config switch or parser-local fallback.
- Keep parser promotion commits separate from monitor-only descriptors.
- Land TUI foundation and individual tabs in small slices so `llmusage dash` can fall back to the existing panel renderer until parity is reached.
