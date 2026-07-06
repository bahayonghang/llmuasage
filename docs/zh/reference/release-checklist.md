# 发布检查清单

在发布 crate、打 tag 或更新公开文档前使用这份清单。

## 版本同步

- `Cargo.toml` 的 package `version` 与 `Cargo.lock` 中 `llmusage` package 条目一致。
- `README.md`、`README.zh-CN.md`、`docs/index.md`、`docs/zh/index.md`、`docs/reference/cli.md`、`docs/zh/reference/cli.md` 都写着同一个 crate 版本。
- `CHANGELOG.md` 已有本次发布条目；如果本次有意不更新 changelog，需要记录这个决定。

## Schema 与 migration 说明

- `src/store/migrations.rs::latest_schema_version()` 对应 `MIGRATIONS` 中最新的真实 migration。
- ADR 0004 与安全文档仍然描述当前 migration 和备份行为。
- 任何 schema 或 pricing metadata 变更，都说明用户是否需要运行 `sync`、`sync --rebuild`，或无需操作。

## 可复现质量门

- 运行 `cargo fmt --check`。
- 运行 `cargo clippy --all-targets --all-features -- -D warnings`。
- 运行 `cargo test -- --test-threads=1`。
- 在 PowerShell 运行 `$env:RUSTDOCFLAGS = "-D warnings"; cargo doc --no-deps`；在 POSIX shell 运行 `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`。
- 在干净 checkout 或 CI runner 上运行 `npm ci --prefix docs`。
- 运行 `npm --prefix docs run docs:build`。

## 本地数据安全

- 不提交 `~/.llmusage/`、SQLite 数据库、`target/`、`docs/node_modules/` 或 VitePress cache/dist 输出。
- 如果改动触及 `sync --rebuild`、`Store::reset_for_source` 或 `Store::reset_usage_data`，重新检查 rebuild/reset 发布说明。
