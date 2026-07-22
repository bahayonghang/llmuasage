# TUI Presentation Contracts

## Scenario: Theme, copy, and terminal color compatibility

### 1. Scope / Trigger

- Apply this contract when changing `src/tui/theme.rs`, interactive TUI copy,
  semantic styles, shared presentation formatting, or terminal color detection.
- CLI report-table ANSI output is a separate surface; sharing pure formatters
  must not change its text or `ColorMode` behavior.

### 2. Signatures

```text
Theme::ALL = [dark, mocha, graphite, lagoon]
TerminalColorMode::from_env() -> TrueColor | Ansi16 | NoColor
theme::configure_from_env()
theme::{fg_style, bold_style, bold_fg_style, selection_style}(...) -> Style
tui::format::{grouped, tokens, footer_compact, axis_compact, stat_compact,
              token_compact, cost, percent_ratio, metric_value}(...)
```

### 3. Contracts

- `dark` is the historical default palette. With no relevant environment
  variables, every semantic slot and style remains identical to the pre-fallback
  TUI.
- `LLMUSAGE_THEME` selects a name from `Theme::ALL`; an absent or unknown name
  uses `dark`. `t` cycles in `Theme::ALL` order and preserves the active terminal
  color mode.
- `NO_COLOR` presence or a truthy `LLMUSAGE_NO_COLOR` selects `NoColor`.
  Truthy values match CLI behavior: non-empty except `0`, `false`, `no`, or
  `off`, case-insensitively.
- `NoColor` removes foreground, background, and modifiers from all nine panels,
  nav/footer, source picker, and help dialog. Borders and text remain present.
- `COLORTERM`/`TERM` values containing `truecolor`, `24bit`, or `direct` select
  `TrueColor`. A known terminal without those markers selects `Ansi16`; no
  capability variables preserves the historical `TrueColor` default.
- `Ansi16` maps every RGB theme slot to the nearest ANSI16 color before render.
- Panel and dialog code uses semantic theme accessors and centralized style
  constructors. `Color::*` is allowed in `theme.rs` and the independent CLI
  `report_table.rs`, not in panels or the source picker.
- Interactive TUI copy is English. The Chinese README/docs surface is
  independent and must not be changed solely for TUI copy normalization.
- Shared format helpers preserve their named output contracts; do not merge
  helpers with different thresholds, precision, or suffix casing.
- `stat_compact` is the interactive analytics formatter. Absolute values below
  1,000 stay exact; larger values use decimal `K/M/B/T`, at most one fractional
  digit, no trailing `.0`, and promote when rounding would produce `1000` of a
  lower unit. It handles signed `i64` values including `i64::MIN`.
- Overview, footer, Models, Daily, Hourly, Cost, Stats, Behavior, and Blocks use
  `stat_compact` for token and analytical count values. Usage sync counters stay
  exact and grouped because scans, inserts, stored events, and skipped files are
  reconciliation evidence. Cost, percentage, timestamp, JSON, web, statusline,
  and CLI report-table formats remain independent.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| `NO_COLOR` exists | `NoColor`, regardless of `TERM`/`COLORTERM` |
| truthy `LLMUSAGE_NO_COLOR` | `NoColor` |
| `LLMUSAGE_NO_COLOR=0|false|no|off` | Continue capability detection |
| `COLORTERM=truecolor` | Preserve RGB theme slots |
| `TERM=xterm-256color`, no truecolor marker | Map RGB slots to ANSI16 |
| no capability variables | Preserve default truecolor behavior |
| unknown `LLMUSAGE_THEME` | Use `dark` without failing startup |
| panel-local `Color::*` added | Source guard test fails |
| Chinese interactive copy added | TUI language guard fails |
| `999`, `1_000`, `12_500` | `999`, `1K`, `12.5K` |
| `999_950`, `999_950_000` | Promote to `1M`, `1B`; never render `1000K/M` |
| Usage sync count `8_000` | Keep exact grouped output `8,000` |

### 5. Good/Base/Bad Cases

- Good: `NO_COLOR=1 llmusage dash` renders readable borders/text with every
  buffer cell at reset foreground/background and no modifiers.
- Good: `TERM=xterm-256color LLMUSAGE_THEME=lagoon llmusage dash` renders the
  Lagoon semantic palette using ANSI16 colors only.
- Base: `llmusage dash` with no color variables renders historical dark colors.
- Good: Overview renders `18_214_785_227` as `18.2B` in both wide and narrow
  layouts while sorting and calculations still use the original integer.
- Bad: returning `Color::Reset` while retaining `Modifier::BOLD`; that is still
  styling and violates `NoColor`.
- Bad: a panel uses `Color::Yellow` directly, so theme switching recolors only
  part of the interface.
- Bad: compacting Usage sync counters, which hides exact reconciliation deltas.

### 6. Tests Required

- Assert default dark semantic slots and centralized styles equal the historical
  colors/modifiers.
- Render all nine panel shells plus source/help dialogs through `TestBackend` for
  every theme and assert the selected accent reaches each surface.
- Render the same surfaces in `NoColor` and assert every cell has reset fg/bg and
  an empty modifier set.
- Unit-test environment detection and assert ANSI16-adapted themes contain no
  `Color::Rgb` slots.
- Scan panel/source-picker source for `Color::*` and interactive TUI source/tests
  for Chinese UI strings.
- Unit-test compact thresholds, rounding promotion, negatives, and signed
  extremes. Render screenshot-scale Overview data through wide and narrow
  `TestBackend` layouts, and keep representative panel plus Usage exact-count
  regression coverage.
- Run strict clippy and `cargo test -- --test-threads=1`.

### 7. Wrong vs Correct

#### Wrong

```rust
Span::styled(value, Style::default().fg(Color::Yellow).bold())
```

This bypasses the active theme and leaves a modifier behind in no-color mode.

#### Correct

```rust
Span::styled(value, theme::bold_fg_style(theme::warning_fg()))
```

The semantic slot follows theme adaptation, and the centralized constructor
returns `Style::default()` in `NoColor` mode.

For numeric presentation, keep analytical readability separate from operational
reconciliation:

```rust
// Wrong: lifetime analytics stay visually noisy, while sync evidence loses precision.
let total = grouped(overview.total.total_tokens);
let inserted = stat_compact(sync.events_inserted);

// Correct: compact analytics, preserve exact sync counters.
let total = stat_compact(overview.total.total_tokens);
let inserted = grouped(sync.events_inserted);
```
