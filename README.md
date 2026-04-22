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

- `llmusage init`
- `llmusage sync`
- `llmusage status`
- `llmusage diagnostics`
- `llmusage doctor`
- `llmusage serve`
- `llmusage tui`
- `llmusage export html`
- `llmusage uninstall`

Development:

```powershell
cargo check
cargo test
cargo run -- init
cargo run -- sync
cargo run -- serve
```

Notes:

- `serve` only binds to `127.0.0.1`
- `export html` generates an offline static report
- `status`, `diagnostics`, and `doctor` are read-only commands
