# Install and initialize

## Install from the repository

```powershell
just install
```

The `just install` task installs the VitePress docs dependencies and installs the CLI from the current checkout.

For development without installation, use `cargo run --`:

```powershell
cargo run -- --help
cargo run -- sync --source codex
```

## Update an installed copy

The self-update command uses the local Rust/Cargo toolchain to build and install
the selected official branch:

```powershell
llmusage update --check
llmusage update
llmusage update dev
```

The default channel is `main`. Before starting Cargo, llmusage shows the current
version, official repository, selected channel, and equivalent install command,
then asks for confirmation. `--check` / `-c` stops after this preview. Use `dev`
only when you intentionally want unreleased changes; it may be less stable or
temporarily fail to build.

The equivalent stable-channel command is:

```powershell
cargo install --git https://github.com/bahayonghang/llmuasage llmusage --branch main --locked --force
```

## Initialize llmusage

```powershell
llmusage init
```

`init` is a local setup command. It prepares the runtime root, bootstraps the database, writes hook wrapper scripts, and installs or probes supported integrations.

## Supported integrations

| Source | Integration surface | Parsed local data |
| --- | --- | --- |
| Codex | `config.toml notify` | OpenAI Codex rollout/session JSONL |
| Claude | `Stop` / `SessionEnd` hooks | Claude Code project JSONL |
| OpenCode | `session.updated` plugin event | OpenCode local SQLite usage database |
| Antigravity | Antigravity `Stop` hook in `~/.gemini/config/hooks.json` | Hook trigger metadata only; transcript import is intentionally absent until a verified schema exists |

If a tool is not installed on the machine, llmusage records the probe/install state and continues with the sources it can see. The Google local CLI source id is `antigravity`; `gemini` is not accepted as a source id. During init/uninstall, llmusage best-effort removes only llmusage-owned legacy `--source gemini` hook commands while preserving user hooks.

## Runtime root precedence

The runtime root is resolved in this order:

1. `--home <PATH>`
2. `LLMUSAGE_HOME`
3. `~/.llmusage`

Examples:

```powershell
llmusage --home .\.tmp-llmusage init
$env:LLMUSAGE_HOME = "D:\tmp\llmusage-home"
llmusage status
```

## Verify setup

```powershell
llmusage status
llmusage doctor
```

`status` summarizes the local database and integrations. `doctor` runs read-only health checks unless you explicitly pass `--refresh-pricing <file>`.
