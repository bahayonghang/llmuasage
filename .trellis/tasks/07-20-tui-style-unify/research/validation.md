# Validation

## Stage one

- Commit: `390b83a refactor(tui): [AI] ♻️ 统一主题槽位与格式化实现`
- `cargo test tui::format --lib`: passed.
- `cargo test tui::theme --lib`: passed.
- `cargo test --test tui_panels_prop -- --test-threads=1`: 25 passed.
- `cargo clippy --lib --tests --all-features -- -D warnings`: passed.
- Default-dark semantic slot assertions preserve every migrated historical
  `Color::*` value. Shared formatter tests preserve each prior precision,
  threshold, grouping, and suffix contract.
- Source guard scans all panels and the source picker; no local `Color::*`
  remains.

## Stage two

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test -- --test-threads=1`: passed.
  - Library: 353 run, 351 passed, 2 ignored.
  - Integration and doc-test suites: all passed.
- `git diff --check`: passed.
- Copy guard scans interactive TUI source and `tests/tui_panels_prop.rs`; no Han
  characters remain. The intended copy changes are listed in
  `copy-changes.md`.
- `truecolor_style_helpers_preserve_stage_one_styles` proves centralized
  truecolor styles have the same fg/bg/modifier values as stage one.
- `every_theme_reaches_all_panel_shells_and_dialogs` renders dark, mocha,
  graphite, and lagoon through `TestBackend` across all nine panels plus source
  and help dialogs; each surface receives the active theme accent.
- `no_color_dashboard_buffers_have_no_styles` renders the same surface set and
  asserts every buffer cell has reset foreground/background and no modifiers.
- Terminal detection tests cover `NO_COLOR`, truthy/false
  `LLMUSAGE_NO_COLOR`, truecolor markers, ANSI16 fallback, and the historical
  no-env default. ANSI16-adapted theme assertions contain no RGB slots.

The TestBackend surface loop replaces an interactive terminal walkthrough with
deterministic cell-level evidence; no manual TTY session was run.
