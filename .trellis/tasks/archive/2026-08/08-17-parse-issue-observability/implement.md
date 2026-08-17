# Implement: parse issue taxonomy

## Order

1. Domain model and ParseIssues tests
2. BoundedJsonlReader oversized prefix callback
3. Codex peek and prefix recovery
4. Zcode, Antigravity, Grok classification
5. CLI sync summary, doctor, source-status
6. SyncSourcePayload, dashboard, TUI
7. Contracts, docs, regression tests

## Validation

- cargo test parsers::file_state parsers::codex parsers::zcode parsers::antigravity -- --test-threads=1
- cargo test commands::sync_summary commands::doctor commands::source_status -- --test-threads=1
- cargo test --test sync_regression -- --test-threads=1
- cargo test web:: -- --test-threads=1
- python scripts/ci-rust.py after the last slice

## Risky files

- src/domain/models.rs
- src/parsers/file_state.rs
- src/parsers/codex.rs
- src/parsers/zcode.rs
- src/query/mod.rs SyncSourcePayload
- src/web/assets/render/sync-command-center.js
- src/tui/panels/usage.rs
- .trellis/spec/llmusage/backend/source-sync-contracts.md

Rollback: revert the branch; parse_issues_json with extra fields remains readable by old binaries if we only add serde-default fields. New kind values in samples would break old readers; keep writing only known kinds.
