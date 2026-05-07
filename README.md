# llmusage

[简体中文](./README.zh-CN.md)

Local-first Rust CLI for AI coding usage analytics.

The goal is simple: use hooks and a local SQLite database to track multiple AI coding CLIs without upload, login, or any cloud API.

Thanks to [vibeusage](https://github.com/victorGPT/vibeusage) for the original idea. `llmusage` is a Rust rewrite and improvement built on that foundation, with a stricter local-first path.

Current v1 coverage:

- Codex `config.toml notify`
- Claude `Stop` / `SessionEnd` hooks
- OpenCode `session.updated` plugin event

Core sources of truth:

- Config directory: `~/.llmusage/`
- Database: `~/.llmusage/llmusage.db`
- Hook wrappers: `~/.llmusage/bin/llmusage-hook.cmd`, `~/.llmusage/bin/llmusage-hook.sh`

Commands:

Report-first commands (read local SQLite only; run `llmusage sync` first if data looks stale):

- `llmusage` / `llmusage daily`
- `llmusage monthly`
- `llmusage session`
- `llmusage blocks`
- `llmusage statusline`

`llmusage` / `llmusage daily` defaults to today's report in the selected timezone. Use `--all` for full daily history, or `--since YYYYMMDD` / `--until YYYYMMDD` for an explicit range.

Common report options include `--since YYYYMMDD`, `--until YYYYMMDD`, `--json`, `--breakdown`, `--order asc|desc`, `--timezone UTC|local|+08:00`, `--locale en-US|zh-CN|ja-JP`, `--compact`, and `--source codex|claude|opencode`.

Operational commands:

- `llmusage init`
- `llmusage sync` (`--rebuild` reparses local sources and rebuilds usage rows/buckets)
- `llmusage status`
- `llmusage diagnostics`
- `llmusage doctor`
- `llmusage serve`
- `llmusage tui`
- `llmusage export html`
- `llmusage uninstall`

Web dashboard:

Below is the local browser dashboard served by `llmusage serve`.

![llmusage web dashboard overview](./docs/public/screenshots/web-dashboard-overview.png)

Development:

```powershell
cargo check
cargo test
cargo run -- init`r`ncargo run -- sync`r`ncargo run -- --json`r`ncargo run -- serve
```

Notes:

- `serve` only binds to `127.0.0.1` and opens the dashboard in your default browser
- `export html` generates an offline static report
- report commands are read-only SQLite views and do not auto-sync`r`n- `status`, `diagnostics`, and `doctor` are read-only commands
