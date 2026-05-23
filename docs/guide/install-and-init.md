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
| Antigravity / Gemini | Antigravity `Stop` hook plus legacy Gemini `SessionEnd` hook | Antigravity hook trigger metadata; legacy `~/.gemini/tmp/*/chats/session-*.json` import |

If a tool is not installed on the machine, llmusage records the probe/install state and continues with the sources it can see. The Google source keeps `gemini` as its stable CLI/API/SQLite id; Antigravity is installed through `~/.gemini/config/hooks.json` when an Antigravity config directory is present, while legacy Gemini CLI remains supported.

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
