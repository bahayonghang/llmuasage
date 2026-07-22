# First report

Report commands read the local SQLite database only. They do not trigger sync.

## Daily report

```powershell
llmusage
llmusage daily
```

With no subcommand, `llmusage` is equivalent to `llmusage daily`. The default daily report shows the last 7 calendar days in the selected timezone, including today. `--timezone local` uses the machine's current fixed local offset; use an explicit fixed offset such as `--timezone +08:00` when you need reproducible historical grouping across machines.

Useful filters:

```powershell
llmusage daily --all
llmusage daily --since 2026-05-01 --until 20260518
llmusage daily --source codex
llmusage daily --project my-repo
llmusage daily --breakdown
llmusage daily --json
```

`--since` and `--until` both accept `YYYYMMDD` and `YYYY-MM-DD`. Daily, weekly, and monthly human tables use a shared coding-agent view: each period has an aggregate `All` row and source rows in the `Agent` column. CLI JSON uses camelCase fields; add `--by-agent` to include the nested source rows in JSON.

Use `--no-cost` to hide cost columns and cost JSON fields without changing the token totals.

## Weekly report

```powershell
llmusage weekly
llmusage weekly --since 2026-05-04 --until 2026-05-10
```

Weekly periods use the Monday date that starts the week. It accepts the same report filters and JSON options as monthly reports.

## Monthly report

```powershell
llmusage monthly --breakdown
```

Monthly uses the same local usage source and supports date range, source, JSON, and compact layout options.

## Combined periods

```powershell
llmusage daily --sections weekly,monthly,session
llmusage monthly --sections daily,session --json
```

`--sections` combines the current period with requested sections in fixed order: the current command period first, then daily, weekly, monthly, and session. The JSON object uses the same order and ends with the current command period's `totals`.

## Session report

```powershell
llmusage session
llmusage session --id <session_id>
llmusage session --project my-repo
```

Session reports use source session metadata when present. Older databases can fall back to stable source-file keys.

## Focused source reports

```powershell
llmusage claude daily
llmusage codex monthly --json
llmusage opencode weekly --no-cost
llmusage antigravity session
```

`claude`, `codex`, `opencode`, and `antigravity` are source hosts. Each supports `daily`, `weekly`, `monthly`, and `session`, with the same data as `<period> --source <source>`. Focused text and JSON remove the Agent comparison layer; JSON has no `agent` or `agents` fields. Passing the same `--source` is accepted, while a conflicting source is rejected. `blocks` is intentionally not available under a source host.

This is a uniform llmusage report surface, not a claim that every source has the same ccusage-specific capability matrix.

## Blocks report

```powershell
llmusage blocks --active
llmusage blocks --recent
llmusage blocks --token-limit max
```

`blocks` builds 5-hour usage windows and burn-rate projections. Change the window with `--session-length <hours>`.

## Statusline

```powershell
llmusage statusline
llmusage statusline --no-cache
```

`statusline` prints one status-bar-friendly line. It may write a tiny local cache under `~/.llmusage/statusline-cache/` unless `--no-cache` is set.
