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
llmusage daily --since 20260501 --until 20260518
llmusage daily --source codex
llmusage daily --project my-repo
llmusage daily --breakdown
llmusage daily --json
```

The human table uses aggregate ccusage-style columns: `Date`, `Models`, `Input`, `Output`, `Cache Create`, `Cache Read`, `Total Tokens`, and `Cost (USD)`.

## Monthly report

```powershell
llmusage monthly --breakdown
```

Monthly uses the same local usage source and supports date range, source, JSON, and compact layout options.

## Session report

```powershell
llmusage session
llmusage session --id <session_id>
llmusage session --project my-repo
```

Session reports use source session metadata when present. Older databases can fall back to stable source-file keys.

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
