# Implementation Plan

Do not run `task.py start` until the user reviews this plan and approves implementation. This task is intentionally split so each phase can be validated independently.

## Phase 0. Baseline and Reference Matrix

- Record current behavior:
  - `cargo test -- --test-threads=1` or targeted parser/TUI tests if full test time is too high.
  - `llmusage source-status` / current source status output.
  - Current `llmusage dash` navigation/render behavior.
- Create a tokscale reference matrix in task notes or `research/tokscale-matrix.md`:
  - client id, root/path pattern, parse-local flag, token quality evidence, parser availability, and llmusage action.
- Decide initial parser-promotion candidates based on available fixtures. Recommended first candidates: Antigravity status/parser clarification, Gemini if real sanitized JSONL exists, and OpenCode path discovery improvements.

Validation:

- Existing tests pass before code changes.
- Reference matrix distinguishes `monitor-only` from `parser-ready`.

## Phase 1. Platform Monitoring Registry

- Add a monitor descriptor model separate from `SourceKind`.
- Seed descriptors from tokscale's known clients and `llmusage` existing sources.
- Add probe helpers for home/env/XDG roots, file patterns, SQLite DB paths, and explicit extra roots.
- Extend `source-status` or add a doctor-style output section showing:
  - detected/unavailable,
  - parser registered/planned/blocked,
  - token quality,
  - privacy class,
  - next action.
- Add tests for descriptor uniqueness, existing source mapping, and monitor-only platforms not appearing as usage sources.

Validation:

- `cargo test source_status -- --test-threads=1`
- Descriptor registry tests.
- Manual status output review with no local artifacts and with temp fixture roots.

## Phase 2. Scanner, Inventory, and Cache Improvements

- Extract reusable inventory helpers from existing parser enumeration code.
- Add extra scan path settings only where needed by existing sources first:
  - OpenCode explicit DB paths and channel DB discovery.
  - Codex/Claude extra roots if repository/user evidence justifies it.
- Add additive cache/fingerprint metadata:
  - size,
  - mtime,
  - sample/full hash as needed,
  - parser version,
  - token-semantics version,
  - append-only offset/prefix hash for safe JSONL incremental parsing.
- Wire cache-hit/cache-miss/skipped counts into `SourceSyncStats` or source sync status.
- Keep parser commits through existing `SyncShard` writer.

Validation:

- Existing Codex, Claude, OpenCode parser tests.
- New sync-twice tests showing unchanged artifacts are skipped and usage is not duplicated.
- Rewritten-file test showing fingerprint invalidation reparses safely.
- OpenCode DB discovery tests for default, channel-suffixed, sidecar, and explicit path cases.

## Phase 3. Parser Promotion Gate for Missing Platforms

- For each candidate parser, require:
  - sanitized fixture,
  - token field mapping document,
  - parser test,
  - sync-twice/cursor test,
  - status/probe test,
  - docs update.
- Promote only parser-ready platforms to `SourceKind` and `registered_parsers()`.
- Keep all other platforms monitor-only with `blocked_no_samples` or `planned`.

Recommended order:

1. Antigravity: resolve current integration-without-parser status and make monitoring explicit.
2. Gemini: only if fixture/token semantics are available.
3. Cursor/Copilot/Zed/Goose/Kiro: monitor first, parser later per evidence.
4. Roo/Kilo/Cline/Codebuff/Crush/Grok/Kimi/Qwen/Warp/Amp/Hermes/Trae: monitor-only unless local samples are supplied.

Validation:

- Per-parser fixture tests.
- `cargo test --test <new_parser_test> -- --test-threads=1`
- Registry coverage tests proving descriptors, parsers, integrations, and monitor descriptors stay consistent.

## Phase 4. TUI Foundation

- Refactor `src/tui` into app/event/data/render layers similar to tokscale:
  - `TuiConfig`,
  - `TuiApp`,
  - event handler with tick/key/mouse/resize,
  - data loader over `Dashboard`, source monitor status, and sync job state,
  - theme registry,
  - reusable header/footer/widgets/dialog stack.
- Add source filter state and source picker dialog.
- Add refresh/sync action that calls existing sync/job contracts.
- Preserve a fallback path to current panel renderers until migrated tabs pass tests.

Validation:

- TestBackend render tests for header/footer at normal and narrow widths.
- Input tests for tab, shift-tab, digits/hotkeys, scroll, sort, quit, and source picker toggles.
- Smoke run `cargo run -- dash` locally after implementation reaches interactive state.

## Phase 5. TUI Tab Migration

- Migrate tabs in this order:
  1. Header/footer/theme/navigation shell.
  2. Overview.
  3. Usage/Sync status.
  4. Models.
  5. Daily with detail drilldown.
  6. Hourly table/profile.
  7. Stats.
  8. Agents/Behavior plus source/health integration.
- Keep each migrated tab backed by `Dashboard` or a small query adapter.
- Add responsive labels and layout tests as each tab lands.
- Add mouse click areas only after keyboard behavior is stable.

Validation:

- `cargo test tui -- --test-threads=1`
- Existing `tests/tui_panels_prop.rs`
- New snapshot-light assertions using TestBackend text extraction, not brittle full-frame snapshots.

## Phase 6. Docs and Final Quality Gate

- Update docs for:
  - monitored vs parsed platforms,
  - token quality labels,
  - cache/fingerprint behavior,
  - `dash` controls and source picker,
  - any changed CLI flags/settings.
- Run targeted tests after each phase and full gate at the end:

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --test-threads=1
npm --prefix docs run docs:build
just ci
```

If `just ci` is too slow or blocked by environment, record the exact command failure and the targeted tests that did pass.

## Risky Files and Rollback Points

- `src/domain/models.rs`: adding `SourceKind` is persistent and should happen only for parser-ready sources.
- `src/domain/source_descriptor.rs`: keep existing descriptors backward compatible.
- `src/registry.rs`: parser registration changes directly affect sync imports.
- `src/store/*`: cache/schema migrations must be additive and tested.
- `src/parsers/*`: never mix parser promotion with TUI-only changes.
- `src/tui/*`: migrate shell first, then tabs one at a time.
- `docs/agents/passive-source-candidates.md`: update when platform monitoring/parser status changes.

Rollback strategy:

- Revert parser-promotion commits independently from monitor descriptors.
- Disable cache reads per source if fingerprint logic is wrong.
- Keep the old TUI shell reachable until the new shell passes navigation/render tests.

## Review Gate Before Implementation

Implementation can start when the user accepts these defaults:

- Missing platforms are monitored first.
- Parsers are added only with fixtures and token-semantics evidence.
- The TUI migrates tokscale's interaction/style while keeping `llmusage` data contracts.
