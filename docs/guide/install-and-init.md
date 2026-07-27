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

The self-update command uses Git plus the local Rust/Cargo toolchain to resolve,
build, and install an official update target:

```powershell
llmusage update --check
llmusage update
llmusage update dev
```

The default `main` channel means the highest stable semantic-version release
tag. llmusage resolves the tag and commit from the official repository, shows
both with the exact install command, and asks for confirmation. It resolves the
target again after confirmation and stops if the target changed. The stable
install pins the displayed immutable commit:

```powershell
cargo install --git https://github.com/bahayonghang/llmuasage llmusage --rev <resolved-sha> --locked --force
```

`--check` / `-c` contacts the official repository to resolve refs and stops
after the preview; it never starts Cargo. Use `dev` only when you intentionally
want unreleased changes. The preview shows the current dev commit, but Cargo
still follows the mutable `dev` branch, which may change or fail to build.

## Initialize llmusage

```powershell
llmusage init
```

`init` is a local setup command. It prepares the runtime root and bootstraps the database. It does not write third-party configuration.

## Passive sources

| Source | Parsed local data | Status |
| --- | --- | --- |
| Codex | OpenAI Codex rollout/session JSONL | Passive parser |
| Claude | Claude Code project JSONL | Passive parser |
| OpenCode | OpenCode local SQLite usage database | Passive parser |
| Kimi Code | Turn-scoped `usage.record` rows | Passive parser |
| Pi / Oh My Pi | Session JSONL from both supported roots | Passive parser |
| Antigravity | Historical database rows only | `historical_only`; no new events until a verified passive schema exists |

The Google local CLI source id remains `antigravity`; `gemini` is not accepted as a source id. Machines upgraded from hook-enabled releases should run `llmusage uninstall` once. Cleanup removes only llmusage-owned legacy commands/plugins/wrappers, preserves sibling user configuration and historical backups, and leaves the usage database intact unless `--purge` is passed.

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

`status` summarizes the local database and sources. `doctor` runs read-only health checks unless you explicitly pass `--refresh-pricing <file>`.
