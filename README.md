# llmusage

[简体中文](./README.zh-CN.md) · [Docs](https://bahayonghang.github.io/llmuasage/)

Local-first usage analytics for AI coding CLIs. `llmusage` reads local Codex, Claude Code, OpenCode, and Google Antigravity artifacts into a local SQLite database, then renders reports, a terminal dashboard, a browser dashboard, and offline HTML exports without upload or login.

> Current crate version: `0.7.1`.

![llmusage web dashboard overview](./docs/public/screenshots/web-dashboard-overview.png)

<small>Dashboard screenshot uses a sanitized local fixture served by `llmusage serve`; it is not real user data.</small>

## Install

From this checkout:

```powershell
just install
```

Or run directly while developing:

```powershell
cargo run -- --help
```

Top-level help is table-oriented for quick scanning. Use `llmusage help --zh` for Chinese help, and `llmusage help <COMMAND>` or `llmusage <COMMAND> --help` for command-specific clap help.

The runtime lives under `~/.llmusage/` by default. Override it with `--home <PATH>` or `LLMUSAGE_HOME`.
Structured runtime logs are local-only NDJSON at `~/.llmusage/logs/llmusage.ndjson`. Control file logging with `LLMUSAGE_LOG=off|error|warn|info|debug|trace` (default: `warn`); `RUST_LOG` continues to control console stderr logging.

## Fast path

```powershell
llmusage init
llmusage sync
llmusage
llmusage serve
```

What this does:

1. `init` creates `~/.llmusage/`, bootstraps `llmusage.db`, writes hook wrappers, and installs supported local integrations.
2. `sync` parses local sources incrementally and writes usage rows, 30-minute buckets, source-file diagnostics, and behavior facts.
3. `llmusage` shows the default daily report for the last 7 calendar days.
4. `serve` starts the local dashboard on `127.0.0.1`.

## Supported local sources

| Source | Local artifacts |
| --- | --- |
| Codex | OpenAI Codex rollout/session JSONL and `config.toml notify` |
| Claude | Claude Code project JSONL plus `Stop` / `SessionEnd` hooks |
| OpenCode | OpenCode local SQLite usage database plus `session.updated` plugin event |
| Antigravity | Antigravity CLI `Stop` hook in `~/.gemini/config/hooks.json` (`--source antigravity`); no transcript parser is registered until a verified token-bearing schema exists |

`source-status` and `dash` also show monitor-only platform candidates such as Gemini CLI, Cursor, Copilot, Zed, Kiro, Goose, Grok, Kimi/Qwen, Roo/Kilo/Cline, Codebuff, Crush, Warp/Oz, Amp, Hermes, and Trae. Monitor-only means llmusage can probe candidate local roots and explain why parsing is blocked; it does not write zero usage rows or untrusted token rows.

## Common commands

```powershell
llmusage daily --source codex --since 20260501 --until 20260518
llmusage monthly --breakdown
llmusage session --project my-repo
llmusage blocks --active
llmusage help --zh
llmusage dash
llmusage logs --limit 50 --level warn
llmusage export html --out .\llmusage-report
```

Report commands are read-only SQLite queries; run `llmusage sync` when the database is stale.

`llmusage dash` uses a tokscale-style terminal dashboard. Keyboard controls: `tab`/`shift-tab` or `1`-`8` switch views, `s` opens the source picker, `r` refreshes dashboard data, `R` toggles auto-refresh, `x` runs sync for the current source filter, `?` opens help/settings, and `q` exits.

The browser dashboard includes behavior panels and a local Cost Explorer workbench for time × metric × group-by slicing, including tool/non-tool cost attribution and offline snapshot export.

## Safety defaults

- No account login, device token, upload queue, or remote usage API call.
- Normal `llmusage sync` keeps imported usage when original source files are missing.
- `llmusage sync --rebuild` refuses lossy rebuilds unless you also pass `--allow-lossy-rebuild`.
- `llmusage diagnostics --forget-file <PATH> --source <SOURCE>` is the explicit write path for intentionally ignored source files.
- `llmusage logs` queries local runtime logs and recent command audit rows without changing report stdout or `sync --json-events` stdout contracts.
- `llmusage doctor --refresh-pricing <file>` reads a local pricing snapshot; URLs are refused.

## Documentation

- [Guide](./docs/guide/getting-started.md)
- [Dashboard](./docs/dashboard/index.md)
- [CLI reference](./docs/reference/cli.md)
- [Safety](./docs/safety/index.md)
- [Architecture](./docs/architecture/index.md)

Development gate:

```powershell
just ci
```
