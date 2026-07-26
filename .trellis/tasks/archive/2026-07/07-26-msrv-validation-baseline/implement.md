# Implementation Plan: Honest MSRV And Validation Baseline

- [x] 用隔离 target 复现 1.85、1.88 及 1.89-1.95 all-features check 并记录结果。
- [x] 同步真实 MSRV metadata、workflow 和 docs。
- [x] 运行 `cargo fmt` 并检查格式 diff。
- [x] 单独和全套复现 subprocess failure，增加 path/error diagnostics。
- [x] 使用公共 rolling-log reader 并保留 NDJSON 行为断言。
- [x] 运行 MSRV check、`cargo fmt --check`、完整 Rust tests 与 `just ci`。

## Verification Evidence

- `cargo test --locked --all-features --test report_commands -- --test-threads=1`: 22 passed.
- `cargo test --locked --all-features --test sync_regression -- --test-threads=1`: 42 passed.
- `python scripts/ci-rust.py`: passed, including 504 unit tests and all integration/doc tests.
- `cargo +1.95.0 check --locked --all-features`: passed with a unique temporary target.
- `just ci`: passed, including dashboard JavaScript checks and VitePress build.
