# Report CLI Contracts

## Scenario: Unified and focused report commands

### 1. Scope / Trigger

- Apply this contract before changing `src/commands/report_args.rs`,
  `src/commands/{daily,weekly,monthly,session,focused,unified_report}.rs`,
  `src/tui/report_table.rs`, or `tests/report_commands.rs`.
- These commands have two consumers: human tables and CLI JSON. They are
  presentation projections over the shared query report types; dashboard,
  export, and interactive-TUI serialization must remain unchanged.

### 2. Signatures

```text
llmusage daily|weekly|monthly|session [REPORT OPTIONS]
llmusage <claude|codex|opencode|antigravity> <daily|weekly|monthly|session> [REPORT OPTIONS]

ReportCommonArgs:
  --since/--until <YYYY-MM-DD|YYYYMMDD>
  --source <SOURCE>
  --json --breakdown --compact --no-cost

UnifiedReportArgs:
  --by-agent
  --sections <daily|weekly|monthly|session,...>

unified_report::focused_report(&UnifiedReport, SourceKind) -> UnifiedReport
unified_report::{report_json, focused_report_json}(..., no_cost) -> JSON
```

### 3. Contracts

- Top-level daily, weekly, monthly, and session commands load through the
  shared unified report query. Daily, weekly, and monthly text use an `All`
  row plus source rows in the `Agent` column; session remains source-row based.
- CLI-only DTOs own camelCase serialization. Query structs retain their
  existing snake_case serialization for dashboard, export, and interactive TUI
  callers.
- `--by-agent` adds nested `agents` only to unified JSON. Human unified tables
  already show Agent rows.
- `--sections` produces an ordered flat JSON object: current command period
  first, then requested daily, weekly, monthly, session periods, then totals
  from the command period. Duplicate section values do not duplicate a field.
- `--no-cost` is an output projection. It removes text cost columns and every
  JSON key containing `cost`, but never changes query filtering or token
  totals.
- `llmusage <source> <period>` injects the matching `ReportFilter.source` and
  uses the shared loader. Focused reports lift the matching source row after
  query loading, then render without `Agent`/`Detected`; focused JSON has no
  `agent` or `agents` keys. All four hosts uniformly support daily, weekly,
  monthly, and session. This is an llmusage extension, not a claim of ccusage
  per-source capability parity.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| `--since` or `--until` uses `YYYYMMDD` or `YYYY-MM-DD` | Parse to the identical inclusive date filter |
| A focused command also has the same `--source` | Accept it |
| A focused command has a different `--source` | Fail with a conflict that names the explicit source |
| Focused `daily --instances` | Fail; project-instance output is not a focused unified projection |
| `daily --instances --sections ...` or `session --id ... --sections ...` | Fail with the existing incompatible-option error |
| Source host requests `blocks` | Clap rejects it; `blocks` remains top-level only |
| Focused source has no matching rows | Render the ordinary empty report state, not an error |

### 5. Good/Base/Bad Cases

- Good: `llmusage codex weekly --json --no-cost` has Codex-only totals, no
  `agent`/`agents`, and no cost keys.
- Good: `llmusage monthly --sections daily,session --by-agent --json` keeps
  `monthly`, `daily`, `session`, then `totals` in that order.
- Base: `llmusage daily --source claude` remains a unified report with an
  `All` row; `llmusage claude daily` is its focused presentation equivalent.
- Bad: serializing `UnifiedRow` directly for CLI JSON. That leaks snake_case
  fields and comparison-only agents to consumers that expect the CLI contract.
- Bad: adding an independent source query for focused reports. It duplicates
  filtering/order/token logic and can drift from `<period> --source <source>`.

### 6. Tests Required

- Unit-test `focused_report` and focused JSON: only the requested source is
  retained; `agent` and `agents` are absent; token totals match that source.
- Parse all four source hosts with all four period subcommands and test same
  versus conflicting `--source` injection.
- Integration-test source/period totals against the matching top-level
  `--source` report. Cover focused text title without `Agent`/`Detected`,
  focused `--sections`, and focused `--no-cost`.
- Keep unified integration coverage for Monday weekly keys, camelCase fields,
  `--by-agent`, both date formats, section order, and no-cost token invariance.
- Run `cargo fmt --check`, strict clippy, focused report tests, and the full
  report command suite before the project-wide gate.

### 7. Wrong vs Correct

#### Wrong

```rust
let report = reports::load_daily_report(&store, &filter)?;
println!("{}", serde_json::to_string_pretty(&report)?);
```

This couples CLI JSON to a shared query payload and makes a focused command
need its own data path.

#### Correct

```rust
let report = reports::load_unified_report(&store, &filter, PeriodKind::Daily)?;
let focused = unified_report::focused_report(&report, source);
println!(
    "{}",
    serde_json::to_string_pretty(&unified_report::focused_report_json(&focused, no_cost)?)?
);
```

The query remains the single source of token/filter/order semantics, while the
CLI-specific DTO enforces the focused output contract.
