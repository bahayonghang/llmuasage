# Codex Tracer

Use `llmusage codex-tracer` when you want a Codex-only dashboard with per-call token accounting and thread-level linkage, separate from the main multi-source `llmusage serve` dashboard.

## What it does

`codex-tracer` reads local Codex rollout JSONL files, writes a dedicated SQLite index, and serves a local browser dashboard.

Default paths:

| Item | Path |
| --- | --- |
| Codex rollout root | `$CODEX_HOME/rollout/` or `~/.codex/rollout/` |
| Tracer database | `~/.llmusage/codex-tracer.db` |
| Local server | `127.0.0.1:8765` |

The tracer database is separate from `llmusage.db`.

## Basic usage

```powershell
llmusage codex-tracer
```

This command:

1. Scans Codex JSONL files under `$CODEX_HOME/rollout/` or `~/.codex/rollout/`.
2. Builds or reuses `~/.llmusage/codex-tracer.db`.
3. Starts a local server on `127.0.0.1:8765`.
4. Opens the dashboard in your default browser unless disabled.

## Common options

```powershell
llmusage codex-tracer --port 9876
llmusage codex-tracer --no-open
llmusage codex-tracer --rebuild
```

| Option | Meaning |
| --- | --- |
| `--port <PORT>` | Bind the local dashboard server to a different port |
| `--no-open` | Start the server without opening a browser |
| `--rebuild` | Delete `codex-tracer.db` and rebuild it from local JSONL files |
| `--home <PATH>` | Override the `~/.llmusage` runtime root, including `codex-tracer.db` |

## When to use it

- Use `llmusage serve` for the main multi-source dashboard.
- Use `llmusage codex-tracer` when you need Codex-specific call details such as cached vs uncached input, reasoning output, and thread call order.

## Requirements and failure modes

- Codex must have produced local rollout JSONL files at least once.
- If the rollout directory does not exist, the command exits with a local-path error and suggests setting `CODEX_HOME`.
- If no events are found, the command exits without starting the dashboard.

## Related references

- [Getting started](./getting-started)
- [CLI reference](../reference/cli)
- [Architecture](../architecture/)
