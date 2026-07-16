# llmusage

[简体中文](./README.zh-CN.md) · [Docs](https://bahayonghang.github.io/llmuasage/)

Local-first usage analytics for AI coding CLIs. `llmusage` reads local Codex, Claude Code, OpenCode, and Google Antigravity artifacts into a local SQLite database, then renders reports, a terminal dashboard, a browser dashboard, and offline HTML exports without upload or login.

> Current crate version: `0.9.2`.

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

On the first sync after an embedded pricing catalog upgrade, `sync` reprices historical events before scanning sources. Stderr reports the catalog versions, processed/total events, bucket reconciliation, and completion. `sync --json-events` exposes the same pricing lifecycle as NDJSON-only stdout.

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
llmusage source-status
llmusage help --zh
llmusage dash
llmusage codex-tracer
llmusage logs --limit 50 --level warn
llmusage catalog status
llmusage export html --out .\llmusage-report
```

Report commands are read-only SQLite queries; run `llmusage sync` when the database is stale.

`llmusage dash` uses a tokscale-style terminal dashboard. Keyboard controls: `tab`/`shift-tab` or `1`-`8` switch views, `s` opens the source picker, `r` refreshes dashboard data, `R` toggles auto-refresh, `x` runs sync for the current source filter, `?` opens help/settings, and `q` exits.

The browser dashboard includes behavior panels and a local Cost Explorer workbench for time × metric × group-by slicing, including tool/non-tool cost attribution and offline snapshot export.

## Pricing catalog

Model pricing and context windows come from the embedded `static-v2` catalog. It includes `gpt-5.6-luna`, `gpt-5.6-terra`, and `gpt-5.6-sol` for Codex and OpenCode, including the exact `gpt-5.6` alias for Sol and request-scoped long-context pricing above 272,000 prompt tokens.

Apply a local incremental overlay without copying the embedded catalog:

```powershell
llmusage catalog apply .\pricing-overlay.json
llmusage catalog status --json
llmusage catalog reset
```

An overlay adds, replaces, or removes complete model definitions by stable model id. Applying or resetting a catalog recomputes persisted event costs and 30-minute bucket pricing. `doctor --refresh-pricing <PATH>` remains the compatibility entrypoint for a complete base snapshot, not an overlay. All catalog inputs are local files; llmusage does not fetch pricing from the network.

Pricing recompute start/reconcile/finish diagnostics are available in the local file log at `LLMUSAGE_LOG=info`; page-level records require `debug`. Human sync progress is shown independently of the file-log level, and a recompute still running after 30 seconds emits one `warn` record at the default level.

## Codex tracer

```powershell
llmusage codex-tracer
llmusage codex-tracer --port 9876
llmusage codex-tracer --no-open
llmusage codex-tracer --rebuild
```

`codex-tracer` is a Codex-only local dashboard. It reads rollout JSONL from `$CODEX_HOME/rollout/` or `~/.codex/rollout/`, builds a separate `~/.llmusage/codex-tracer.db`, and serves a dedicated browser UI with detailed token accounting and thread tracking.

## Safety defaults

- No account login, device token, upload queue, or remote usage API call.
- Normal `llmusage sync` keeps imported usage when original source files are missing.
- `llmusage sync --rebuild` refuses lossy rebuilds unless you also pass `--allow-lossy-rebuild`.
- Legacy token-accounting rows stay readable but block new writes until each parser source is rebuilt with `llmusage sync --rebuild --source <source>`.
- `llmusage diagnostics --forget-file <PATH> --source <SOURCE>` is the explicit write path for intentionally ignored source files.
- `llmusage logs` queries local runtime logs and recent command audit rows without changing report stdout or `sync --json-events` stdout contracts.
- `llmusage catalog apply <file>` and `doctor --refresh-pricing <file>` read local catalog files; URLs are refused.

## Documentation

- [Guide](./docs/guide/getting-started.md)
- [Codex tracer guide](./docs/guide/codex-tracer.md)
- [Dashboard](./docs/dashboard/index.md)
- [CLI reference](./docs/reference/cli.md)
- [Safety](./docs/safety/index.md)
- [Architecture](./docs/architecture/index.md)

Development gate:

```powershell
just ci
```
