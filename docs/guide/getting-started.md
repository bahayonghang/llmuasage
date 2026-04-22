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
llmusage serve
```

### What each step does

- `init` prepares `~/.llmusage/`, creates `llmusage.db`, generates hook wrappers, and installs Codex / Claude / OpenCode integrations.
- `sync` parses local sources incrementally and upserts usage data into SQLite.
- `serve` starts the browser dashboard on `127.0.0.1`.

## Verify the repo

```powershell
just ci
```

`ci` runs format, clippy, tests, and a VitePress production build.
