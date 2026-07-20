# llmusage Backend Guidelines

`llmusage` is the Rust CLI in the repository root. Backend changes usually
span source discovery, passive parsers, SQLite storage, query payloads, CLI
output, and dashboard/TUI consumers.

## Pre-Development Checklist

- Read [Source Sync Contracts](./source-sync-contracts.md) before changing
  source registries, parser sync stats, sync command summaries, source status,
  or dashboard/TUI sync payloads.
- Read [Pricing Catalog Contracts](./pricing-catalog-contracts.md) before
  changing embedded pricing rows, model matcher behavior, cost computation, or
  context-window coverage.
- Read [Token Accounting Contracts](./token-accounting-contracts.md) before
  changing parser token fields, deduplication, totals, costs, or legacy rebuild
  behavior.
- Read [TUI Presentation Contracts](./tui-presentation-contracts.md) before
  changing interactive themes, copy, formatting, or terminal color behavior.
- Also read `docs/agents/domain.md` and
  `docs/agents/passive-parser-onboarding.md` before promoting a monitored
  platform into a parser-backed source.

## Guidelines Index

| Guide                                                 | Description                                                                     | Status     |
| ----------------------------------------------------- | ------------------------------------------------------------------------------- | ---------- |
| [Source Sync Contracts](./source-sync-contracts.md)   | Parser/source monitor boundaries and sync stats payload contracts               | Documented |
| [Pricing Catalog Contracts](./pricing-catalog-contracts.md) | Static pricing rows, model matchers, cost status, and context-window contracts | Documented |
| [Codex Tracer Contracts](./codex-tracer-contracts.md) | Codex-specific usage tracker with detailed token accounting and thread tracking | Documented |
| [Dashboard Performance Contracts](./dashboard-performance-contracts.md) | Interactive payload, query routing, cancellation, and range-refresh budgets | Documented |
| [Token Accounting Contracts](./token-accounting-contracts.md) | Parser normalization, logical dedupe, authoritative totals, and guarded legacy rebuild | Documented |
| [TUI Presentation Contracts](./tui-presentation-contracts.md) | Interactive theme slots, English copy, shared formatters, and terminal color fallback | Documented |

## Quality Check

- Run `cargo fmt --check`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Run the focused Rust test slice for the changed contract.
- Run `cargo test -- --test-threads=1` for cross-layer source/query/TUI changes.
- Run `npm --prefix docs run docs:build` when docs changed.
