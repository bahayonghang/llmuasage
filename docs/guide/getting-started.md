# Getting Started

`llmusage` is a Rust CLI that keeps the entire analytics path local.

## Requirements

- Rust stable toolchain
- Node.js 20+
- npm 10+
- `just`

## Install dependencies

```powershell
just install
```

This does two things:

- installs the VitePress docs dependencies under `docs/`
- installs the CLI from the current checkout with `cargo install --path . --locked --force`

## Run the local flow

```powershell
llmusage init
llmusage sync
llmusage
llmusage serve
```

### What each step does

- `init` prepares `~/.llmusage/`, creates `llmusage.db`, generates hook wrappers, and installs Codex / Claude / OpenCode integrations.
- `sync` parses local sources incrementally and upserts usage data into SQLite.
- `llmusage` without a subcommand prints today's daily report from the local DB. Use `llmusage daily --all` for full history, or `llmusage monthly`, `llmusage session`, and `llmusage blocks` for other report views.
- `serve` starts the browser dashboard on `127.0.0.1` and opens it in your default browser.

Report commands are read-only and never upload data. They also do not auto-sync; run `llmusage sync` again when source data changes. If you need to repopulate new session metadata after upgrading, run `llmusage sync --rebuild`.

## Verify the repo

```powershell
just ci
```

`ci` runs format, clippy, tests, and a VitePress production build.
