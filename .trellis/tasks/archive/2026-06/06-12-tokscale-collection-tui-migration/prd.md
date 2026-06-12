# tokscale Collection and TUI Migration

## Goal

Use `ref/tokscale` as a reference to improve `llmusage` local usage collection and monitoring, then migrate the tokscale terminal dashboard's core interaction model, visual style, and major views into `llmusage` without replacing `llmusage`'s existing SQLite/store/query architecture.

This task has two deliverables:

1. Improve usage collection trustworthiness and performance, including visible monitoring for platforms that `tokscale` knows about but `llmusage` cannot safely parse yet.
2. Rework `llmusage dash` so it behaves like the tokscale TUI in day-to-day use: tabbed dashboard, source picker, refresh/sync visibility, responsive terminal layout, themeable styling, richer daily/hourly/model/stat views, and useful keyboard/mouse interactions.

## Confirmed Facts

### Current `llmusage`

- Stable persisted source ids live in `SourceKind`; current ids are Codex, Claude, OpenCode, and Antigravity.
- `SourceDescriptor` already records activation mode, source capabilities, usage quality, and privacy class.
- `registered_parsers()` currently registers Codex, Claude, and OpenCode parsers. Antigravity has an integration descriptor but no usage parser.
- The parser driver intentionally runs registered parsers serially through one `SyncRunWriter` / SQLite write path and emits `SyncEvent` lifecycle/progress events for CLI JSON, human progress, and `JobRegistry`.
- Existing parsers already use shard plans, cursors, source-file state, and cancellation/progress boundaries. Codex and Claude parse JSONL artifacts; OpenCode reads SQLite pages.
- `llmusage dash` currently has fixed panels (Overview, Trends, Models, Sources, Projects, Cost, Health, Behavior), synchronous key-event reads, lazy panel cache, and no auto-refresh, sync-job controls, source picker, theme picker, mouse support, or settings dialog.
- The web dashboard already has richer sync/job concepts that should be reused as contracts instead of inventing unrelated TUI behavior.

### `ref/tokscale`

- `tokscale` keeps a broad client catalog (`ClientDef`) with ids, roots, relative paths, file patterns, and capability flags such as headless/local-parse/submit-default.
- Its scanner supports environment roots, extra scan paths, per-client filters, OpenCode channel DB discovery, dedupe, and parallel directory walking.
- Its message cache records schema version, atomic save behavior, file fingerprints, SQLite sidecar awareness, and Codex incremental offset/prefix validation.
- Its TUI has tabs for Overview, Usage, Models, Daily, Hourly, Minutely, Stats, and Agents; it includes a themed header/footer, source/client picker, refresh/status affordances, mouse click areas, responsive labels, and test-backed terminal rendering.
- Its parser set covers far more platforms than `llmusage`, but parser trust depends on per-platform token semantics and samples. It should be treated as evidence and inspiration, not copied as an unconditional parser import.

## Requirements

### R1. Collection trustworthiness

- Preserve `llmusage`'s existing `SourceKind -> SourceParser -> SyncShard -> Store -> Dashboard` contract.
- Do not write usage rows for a new platform unless the parser has real or sanitized fixture coverage, documented token semantics, and sync-twice/cursor regression tests.
- Explicitly label usage quality and confidence for every source/platform status shown to users: precise, total-only, estimated, parserless, blocked-no-samples, or unavailable.
- Unsupported platforms must not appear as zero usage. They should appear as monitored candidates with discovery/status/probe information.
- Token channel semantics must stay source-aware: input, output, cache read, cache creation/write, and reasoning tokens must not be silently folded unless the source is explicitly `total_only` or `estimated`.

### R2. Collection performance

- Avoid reparsing unchanged local artifacts when size/mtime/hash/fingerprint evidence says the artifact is unchanged.
- Preserve the single SQLite write boundary while allowing safe parallel inventory/read/parse work before shard commit.
- Surface progress per source and per scan phase so CLI, web, and TUI can distinguish scanning, parsing, committing, skipped unchanged files, and blocked sources.
- Add benchmarks or focused timing tests for high-volume Codex/Claude/OpenCode paths before adopting heavier dependencies such as SIMD JSON.

### R3. Missing platform monitoring

- Add a platform monitoring registry derived from the tokscale client matrix, separate from persisted `SourceKind` where no parser exists yet.
- At minimum, monitor existing `llmusage` sources plus high-value missing platforms visible in tokscale: Gemini, Cursor, Copilot, Zed, Kiro, Goose, Grok, Kimi/Qwen, Roo/Kilo/Cline, Codebuff, Crush, Warp/Oz, Amp, Hermes, Trae, and related OpenCode-compatible variants.
- For each monitored platform, expose display name, candidate roots, discovery status, parser support status, expected privacy class, token-quality status, and next action when parsing is blocked.
- Integrate monitoring into `source-status`/doctor-style CLI output and the migrated TUI source/status views.

### R4. TUI migration

- Rebuild `llmusage dash` around the tokscale-style tabbed app model while using `llmusage` data/query APIs.
- Include the core tokscale views where data exists: Overview, Usage/Sync, Models, Daily, Hourly, Stats, and Agents/Behavior. Keep `llmusage`-specific source, project, cost, and health information either as tabs, subviews, or dialogs.
- Add responsive header/footer, keyboard navigation, scroll/sort controls, source picker, refresh/sync controls, status/error display, theme support, and TestBackend coverage for narrow and normal terminal widths.
- TUI sync actions must go through existing sync/job contracts rather than embedding scanner/parser logic directly inside UI rendering.

## Acceptance Criteria

- [ ] `prd.md`, `design.md`, and `implement.md` exist and describe the collection, monitoring, and TUI migration plan before implementation starts.
- [ ] Existing source ids and stored usage semantics remain backward compatible.
- [ ] New platform monitoring can report detected/parserless/blocked/unavailable states without writing untrusted usage rows.
- [ ] Existing Codex, Claude, and OpenCode imports remain precise and pass existing parser/store tests.
- [ ] Collection changes reduce repeated work for unchanged artifacts and expose skipped/parsed/committed counts in sync status.
- [ ] The terminal dashboard has tokscale-style tabs, header/footer styling, source picker, refresh/sync visibility, responsive layout, and tests for keyboard/mouse/navigation behavior.
- [ ] Documentation explains which platforms are monitored only, which are parsed, and what evidence is required to promote a monitored platform into an imported source.
- [ ] Final validation includes targeted parser/store/TUI tests plus the repo's standard gate (`just ci`) unless explicitly waived.

## Out of Scope

- Copying tokscale's full in-memory aggregation pipeline as the main `llmusage` architecture.
- Adding parsers for every tokscale platform without fixtures and token-semantics evidence.
- Remote usage submission, billing account login flows, or tokscale-specific remote APIs.
- Replacing the existing SQLite store, sync shard protocol, or web dashboard contracts.
- Large unrelated dashboard redesign outside `llmusage dash`.

## Open Decision

Recommended default: implement descriptor/probe/status monitoring for missing platforms first, and promote only fixture-backed platforms into real parsers. Choosing to implement many parsers immediately would expand scope and create a high risk of inaccurate token accounting.
