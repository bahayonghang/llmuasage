# Design

## Boundaries

- `src/tui/panels/overview.rs` owns the Overview layout change.
- `src/commands/dash.rs` owns the deprecated `tui` warning.
- `src/tui/report_table.rs` already owns CLI color mode environment handling;
  expose only the minimal stderr color decision needed by the warning.

## Overview Layout

Use the current `OverviewPayload` only. Keep the existing KPI row, then add:

- `Token Mix`: input, output, cache read/write, and reasoning totals.
- `Recent Activity`: events, 24h events, average tokens per event, source count,
  and bucket count.
- `Freshness`: last sync, last export, generated timestamp, and cache hit rate.
- `24h Pulse`: last-24-hour token mix, event count, average tokens per event,
  and share of all-time tokens.

The detail layout remains bounded to fixed-height rows so very tall terminals
do not stretch text awkwardly. The 24h Pulse block occupies remaining height,
which makes large terminals look intentionally framed instead of blank. Narrow
terminals fall back to stacked sections.

## Warning Styling

Keep the text contract recognizable:

`warning: \`tui\` is deprecated, use \`llmusage dash\` instead`

When stderr color is enabled, style the `warning:` label yellow/bold and the
replacement command cyan/bold. When color is disabled, return the exact plain
text message.

## Compatibility

- No command semantics change.
- No JSON output change.
- No database or dashboard API change.
- Color disabling remains script-friendly through existing env vars.
