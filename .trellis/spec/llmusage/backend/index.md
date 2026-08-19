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
- Read [Report CLI Contracts](./report-cli-contracts.md) before changing
  daily/weekly/monthly/session report arguments, report JSON, source-focused
  commands, or CLI report-table output.
- Read [TUI Runtime Contracts](./tui-runtime-contracts.md) before changing the
  event loop, redraw policy, render snapshots, or scrollable table construction.
- Read [TUI Subscription Contracts](./tui-subscription-contracts.md) before
  changing dash Usage quota fetchers, cache, or the sync overlay.
- Read [Web Server Contracts](./web-server-contracts.md) before changing the
  `serve` listener, browser-launch policy, or dashboard network exposure.
- Read [Self-Update Contracts](./self-update-contracts.md) before changing the
  `update` command, supported channels, confirmation flow, or Cargo invocation.
- Read [CI And Toolchain Contracts](./ci-toolchain-contracts.md) before changing
  dependencies, `rust-version`, Rust CI commands, or subprocess test harnesses.
- Read [Write Fencing Contracts](./write-fencing-contracts.md) before changing
  worker locks, bootstrap, migrations, sync writers, or any Store mutation.
- Read [Integration File Contracts](./integration-file-contracts.md) before
  changing third-party hook/plugin configuration writes or action recording.
- Read [Runtime Log Contracts](./runtime-log-contracts.md) before changing
  structured runtime logging, rotation, retention, tail reads, or log status.
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
| [TUI Runtime Contracts](./tui-runtime-contracts.md) | Dirty redraws, tick coalescing, frame snapshots, and visible-row caches | Documented |
| [TUI Subscription Contracts](./tui-subscription-contracts.md) | Read-only quota fetchers, cache TTL, and Usage overlay | Documented |
| [Report CLI Contracts](./report-cli-contracts.md) | Unified/focused report command surface, DTO projections, and output invariants | Documented |
| [Web Server Contracts](./web-server-contracts.md) | Dashboard listener, browser-launch, SSH, and network-exposure contracts | Documented |
| [Self-Update Contracts](./self-update-contracts.md) | Official channels, Cargo invocation, confirmation, and no-network test boundaries | Documented |
| [CI And Toolchain Contracts](./ci-toolchain-contracts.md) | Shared Rust gate, verified MSRV, and subprocess-test evidence rules | Documented |
| [Write Fencing Contracts](./write-fencing-contracts.md) | Generation permits, transaction fencing, bootstrap ordering, and mutation entrypoints | Documented |
| [Integration File Contracts](./integration-file-contracts.md) | Cross-platform atomic replace, recovery, and integration action recording | Documented |
| [Runtime Log Contracts](./runtime-log-contracts.md) | Bounded runtime-log rotation, retention, counters, and tail reads | Documented |

## Quality Check

- Run `python scripts/ci-rust.py` for the shared format, clippy, Rust test, and
  rustdoc gate.
- Run the focused Rust test slice first for the changed contract.
- Run `cargo test -- --test-threads=1` for cross-layer source/query/TUI changes.
- Run `npm --prefix docs run docs:build` when docs changed.
