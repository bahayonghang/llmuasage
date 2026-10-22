# A bilingual documentation evidence

Date: 2026-10-21. Documentation source frozen for coordinator validation.
Ownership: documentation only; no product code, configuration, lockfiles,
version bumps, installs, commits, or real-data operations by this worker.

## Changed pages and sections

| Page pair | Sections and resulting behavior |
| --- | --- |
| `README.md`, `README.zh-CN.md` | Language discovery before Fast path / 最短路径; runtime-root profile scope and clean exception; Common commands / 常用命令 adds lang inspection and en/zh setting; Reports / 报表 qualifies translated aggregate/source labels and session behavior; dash paragraph explains startup snapshot, r/R/m/y/x controls, reopen, independent browser/desktop/HTML language, canonical machine/historical data and statusline caches. |
| `docs/reference/cli.md`, `docs/zh/reference/cli.md` | Existing version label corrected from 2.0.0 to actual Cargo.toml 2.0.1 without changing package versions; Top-level help / 顶层 help now follows saved language. New Interface language / 界面语言 subsection documents en default, en/zh/zh-CN normalization, profile precedence, clean exception, read-only no-create, malformed settings, pre-init operation, three-stream TTY menu, cancellation, non-TTY zero stdin reads, safe save failure, startup-only TUI language, independent Web/desktop/HTML, and machine/original-detail boundaries. Global --locale is accurately described as report compatibility values. Runtime logging distinguishes human projection from canonical stored events. Statusline documents latest.en.txt/latest.zh.txt isolation. Sync documents ten-column closed borders, grouped post-table diagnostics, notice/file distinctions, recovery keys/default keep/coverage safety. Existing dash section covers nine panels/dialogs, unchanged controls, quota cache/r/R/m, y overlay, credentials, NoColor and startup snapshot. |
| `docs/guide/first-sync.md`, `docs/zh/guide/first-sync.md` | Optional language selection before init; Import all sources / 导入所有来源 uses exact corresponding captured stdout, with isolated fixture provenance and variable measured timing. Adds ten-column geometry, numeric bounds, reader/time measurement limits, grouped capped diagnostics and notice-count semantics. Import one source / 只导入一个来源 corrects old Pi automatic-rebuild claim to preserve/skip and explicit pi repair. NDJSON section documents invariant machine stdout and human stderr. Rebuild safely / 安全重建 adds all coverage categories, k/K/r/R/empty/EOF/read-failure/two-invalid defaults, three-stream TTY/flag restrictions and unreadable/discovery protection. |

`docs/commands/index.md` and `docs/zh/commands/index.md` remain unchanged.
They are intentional five-line legacy URL relocation stubs pointing at the
current CLI reference. Duplicating language documentation there would create
a second maintenance surface. All eight task-owned entry points were reviewed;
six required edits. No new topic page was added.

## Source and contract anchors verified

- `Cargo.toml:3`: package version 2.0.1.
- `src/common/i18n/mod.rs:18`: canonical en/zh and zh-CN alias.
- `src/commands/lang.rs:12`, `:51`, `:63`: explicit save, three-stream
  terminal menu, safe cancellation and non-terminal branch without stdin reads.
- `src/runtime/preferences.rs:50`, `:85`: read-only fallback versus explicit
  independent atomic preference write. Shared CLI language contract specifies
  root precedence and the retained clean exception.
- `src/commands/entrypoint.rs:31`, `:157`, `:166`, `:217`: metadata-based root
  selection, early language read, help/parser output and pre-AppContext lang.
- `src/commands/statusline.rs:40`: selected-language cache path.
- `src/commands/sync.rs:176`: can_prompt requires stdin/stdout/stderr TTY and
  excludes json-events, rebuild and recent-days. `sync_recovery.rs:9` / `:69`
  preserve coverage facts and default-keep behavior in both languages.
- `src/commands/sync_summary.rs:21`, `:74`, `:239`: ten headers, bilingual
  summary/notice rendering and closed borders. Metric scope follows the
  current source-sync contract; no wall-clock or physical-I/O claim is added.
- `src/tui/mod.rs:62`, `:65`, `:121`: direct callers read one Store-root
  snapshot; CLI passes its existing snapshot. `src/tui/input.rs:46` maps the
  original controls including r/R/x/s/y/m. `src/subscription/cache.rs:6`
  retains the 300-second TTL; TUI runtime/subscription contracts retain quota
  entry/r refresh, no R API polling, read-only credentials and hidden email.
- Actual T `research/ui-evidence.md` and `research/check-evidence.md` were read.
  The final coordinator confirmation records 1320 PASS / 19 existing ignored,
  fmt/strict Clippy/rustdoc PASS on the frozen T source; this worker did not
  rerun that Rust gate or substitute it for A acceptance.
- ADR 0015's historical ordinary-sync automatic Pi migration conflicted with
  the current protected policy. This was reported to the coordinator, who
  added a dated superseding annotation while preserving historical/remote
  decisions. The first-sync pages now describe the current source-sync policy.

## Captured examples and local checks

Coordinator-supplied `fixture-sync.en.txt` and `fixture-sync.zh.txt` were
copied verbatim into each matching first-sync fenced text block (newline
normalization only). Both have one Codex event, one changed file, zero skipped
files and 482 B. Captured parse timing is 8ms English and 7ms Chinese; both
write timings are 1ms. These differences remain intact and are explicitly
qualified as variable measurements. The tables count events, not the fixture's
123 tokens. Isolation, inputs, command/environment and cleanup are recorded in
`capture-fixture.py` and `fixture-capture.json`; no capture was rerun here.

Checks run by this documentation worker:

- PASS: scoped `rtk bypass git diff --check` on all six changed pages.
- PASS: Python extracted both fenced text blocks and compared them with the
  corresponding capture after newline normalization; checked no ANSI and
  exactly eleven vertical separators for each ten-cell row.
- PASS: Python checked balanced fences and every relative file link in all
  six changed pages, including extensionless VitePress links.
- PASS: paired read-through against shared language/report/sync/logging/TUI
  contracts and actual T evidence. Commands stubs intentionally unchanged.

## Remaining acceptance boundaries at handoff

- Docs build: NOT_RUN by this worker; coordinator owns
  `npm --prefix docs run docs:build` and the unchanged final `just ci` after
  source freeze. This evidence does not claim final cross-surface PASS.
- Native ordinary-window CLI/TUI English/Chinese smoke: NOT_RUN by this
  worker. Captured subprocess output and TestBackend evidence do not count
  as native terminal acceptance. Coordinator records its separate status.
- No unresolved documentation issue identified at freeze. Source mutations
  after freeze or reviewer findings require reassessing affected checks.
