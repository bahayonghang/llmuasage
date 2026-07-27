# Implementation Plan: Sync Job Contract Closure

- [x] 为 public JobRegistry、CLI 和 Web 增加同表驱动的非法输入测试。
- [x] 引入 validated request 与 stable error code。
- [x] 将 source/all 选择改为显式 enum，移除 unknown-to-None 路径。
- [x] 实现 recent cutoff 传播、保守 discovery pruning 和事件级过滤。
- [x] 修正 `RecentReady` 时序与进度统计。
- [x] 更新 CLI/help/API 文档及 ADR-0005 相关说明。
- [x] 运行 job registry、parser、web contract tests、完整 Rust tests 和 `just ci`。

## Verification Evidence

- `cargo test --all-features --test m2_raw_archive_logs recent_window_filters_old_events_without_advancing_full_history_cursor -- --test-threads=1`
- `cargo test --all-features api_jobs_start_rejects_ -- --test-threads=1`
- `cargo test --all-features --test public_api cli_sync_uses_shared_stable_validation_codes -- --test-threads=1`
- `cargo test --all-features recent_lower_bound_prunes_old_message_rows_in_sql -- --test-threads=1`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `npm --prefix docs run docs:build`
- `just ci`

The first standalone full-test run hit the unrelated 80 ms dashboard benchmark at 81.9 ms (93.0 ms on isolated rerun). The subsequent canonical `just ci` run passed the complete serial Rust suite and all remaining gates.
