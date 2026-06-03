# Release checklist

Use this checklist before publishing a crate, tagging a release, or updating public docs.

## Version synchronization

- `Cargo.toml` package `version` and the `llmusage` package entry in `Cargo.lock` match.
- `README.md`, `README.zh-CN.md`, `docs/index.md`, `docs/zh/index.md`, `docs/reference/cli.md`, and `docs/zh/reference/cli.md` mention the same crate version.
- `CHANGELOG.md` has an entry for the release, or the release intentionally ships without a changelog update and that decision is recorded.

## Schema and migration notes

- `src/store/migrations.rs::latest_schema_version()` reflects the newest real migration in `MIGRATIONS`.
- ADR 0004 and the safety docs still describe the current migration and backup behavior.
- Any schema or pricing metadata change documents whether users need `sync`, `sync --rebuild`, or no action.

## Reproducible quality gate

- Run `cargo fmt --check`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Run `cargo test -- --test-threads=1`.
- Run `$env:RUSTDOCFLAGS = "-D warnings"; cargo doc --no-deps` on PowerShell, or `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` on POSIX shells.
- Run `npm ci --prefix docs` on a clean checkout or CI runner.
- Run `npm --prefix docs run docs:build`.

## Local-data safety

- Do not commit `~/.llmusage/`, SQLite databases, `target/`, `docs/node_modules/`, or VitePress cache/dist output.
- Re-check rebuild/reset release notes when a change touches `sync --rebuild`, `Store::reset_for_source`, or `Store::reset_usage_data`.
