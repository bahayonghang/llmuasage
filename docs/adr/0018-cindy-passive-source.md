# ADR 0018 — Cindy passive source

- Status: Accepted
- Date: 2026-10-25
- Related code: `src/domain/models.rs`, `src/parsers/pi.rs`, `src/parsers/source_files.rs`, `src/registry.rs`, `src/domain/platform_monitor.rs`
- Related terms: Source, SourceParser, SourceDescriptor, FileCursor

## Context

Cindy writes Pi-format session JSONL under its official app profile, not under `~/.pi/agent/sessions`. Pointing `PI_AGENT_DIR` at that directory would hide a real Pi install and still label the rows `pi`.

ccusage treats this shape as an additive named store with its own agent name, and rejects paths that overlap the default Pi store. ADR 0015 already split Oh My Pi from Pi for the same reason.

## Decision

Register `SourceKind::Cindy` (`cindy`, display name `Cindy`) beside Pi and Oh My Pi. Reuse `PiFormatParser`.

- Discovery reads `{config_dir}/Cindy/pi-agent-home/sessions` and `{config_dir}/CindyGlobal/pi-agent-home/sessions`. `config_dir` is the platform config directory. Dev and sandbox profile names are not scanned.
- `LLMUSAGE_CINDY_SESSION_DIRS`, when set and non-empty, replaces those roots. It does not read `PI_AGENT_DIR`.
- Files under `sessions/shared` are imported transcripts and are omitted.
- A Cindy candidate whose canonical path overlaps a Pi or Oh My Pi root is skipped. Pi and Oh My Pi keep their existing ownership rule.
- Token accounting version is `2`. There is no legacy Cindy ledger and no schema migration.
- No focused `llmusage cindy` subcommand. Unified reports and `--source cindy` are enough.

## Consequences

- Ordinary sync imports Cindy without rebuilding Pi or Oh My Pi.
- `source-status` shows the official profile roots. The env override is owned by the parser lister.
- Hardcoded source lists must include `cindy`. Compiler exhaustiveness does not catch those strings.

## Rollback

Revert the registration. Already-written `cindy` rows stay until an explicit source reset. Pi and Oh My Pi cursors are untouched.
