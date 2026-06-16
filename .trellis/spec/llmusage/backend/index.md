# llmusage Backend Guidelines

`llmusage` is the Rust CLI in the repository root. Backend changes usually
span source discovery, passive parsers, SQLite storage, query payloads, CLI
output, and dashboard/TUI consumers.

## Pre-Development Checklist

- Read [Source Sync Contracts](./source-sync-contracts.md) before changing
  source registries, parser sync stats, sync command summaries, source status,
  or dashboard/TUI sync payloads.
- Also read `docs/agents/domain.md` and
  `docs/agents/passive-parser-onboarding.md` before promoting a monitored
  platform into a parser-backed source.

## Guidelines Index

| Guide                                                 | Description                                                                     | Status     |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- | ---------- |
| [Source Sync Contracts](./source-sync-contracts.md)   | Parser/source monitor boundaries and sync stats payload contracts               | Documented |
| [Codex Tracer Contracts](./codex-tracer-contracts.md) | Codex-specific usage tracker with detailed token accounting and thread tracking | Documented |

## Quality Check

- Run `cargo fmt --check`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Run the focused Rust test slice for the changed contract.
- Run `cargo test -- --test-threads=1` for cross-layer source/query/TUI changes.
- Run `npm --prefix docs run docs:build` when docs changed.
