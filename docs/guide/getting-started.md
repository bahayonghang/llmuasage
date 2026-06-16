# Getting started

Use this guide when you want a local database, a first report, and a browser dashboard without learning every command first.

## Requirements

- Rust stable toolchain
- Node.js 20+
- npm 10+
- `just`

## 1. Install from this checkout

```powershell
just install
```

This installs docs dependencies under `docs/` and installs the CLI with `cargo install --path . --locked --force`.

## 2. Initialize local runtime and hooks

```powershell
llmusage init
```

`init` creates the runtime root, bootstraps SQLite, writes hook wrappers, and installs supported integrations for Codex, Claude Code, OpenCode, and Google Antigravity when their local config files are present.

Default paths:

| Item | Path |
| --- | --- |
| Runtime root | `~/.llmusage/` |
| Database | `~/.llmusage/llmusage.db` |
| Hook wrappers | `~/.llmusage/bin/llmusage-hook.cmd`, `~/.llmusage/bin/llmusage-hook.sh` |
| Static exports | `~/.llmusage/exports/` |

Override the runtime root with `--home <PATH>` or `LLMUSAGE_HOME`.

## 3. Import local usage

```powershell
llmusage sync
```

`sync` parses local sources incrementally and writes normalized usage rows, 30-minute buckets, source-file diagnostics, and behavior facts.

Use a source filter when you only want one source:

```powershell
llmusage sync --source codex
```

## 4. Read the default report

```powershell
llmusage
```

With no subcommand, `llmusage` is the `daily` report. It shows the last 7 calendar days in the selected timezone, including today. `--timezone local` uses the machine's current fixed local offset; pass a fixed offset such as `--timezone +08:00` for reproducible historical grouping.

Use JSON for automation:

```powershell
llmusage daily --json --source antigravity
```

## 5. Open local dashboards

Terminal dashboard:

```powershell
llmusage dash
```

Browser dashboard:

```powershell
llmusage serve
```

`serve` binds to `127.0.0.1`, prints the local URL, and tries to open the default browser.

Codex-only browser dashboard:

```powershell
llmusage codex-tracer
```

Use this when you want Codex-specific call and thread details in a dedicated `codex-tracer.db`.

## 6. Export an offline report

```powershell
llmusage export html --out .\llmusage-report
```

The export writes a static dashboard snapshot with `index.html`, `snapshot.json`, and `assets/*`.

## Next steps

- [First sync](./first-sync) for safe rebuild behavior and NDJSON progress.
- [First report](./first-report) for report filters and table semantics.
- [Codex Tracer](./codex-tracer) for the dedicated Codex dashboard and rebuild behavior.
- [Dashboard](../dashboard/) for `llmusage serve` filters, behavior panels, and degraded states.
- [Safety](../safety/) for local data paths and destructive boundaries.
- [CLI reference](../reference/cli) for exact flags.
