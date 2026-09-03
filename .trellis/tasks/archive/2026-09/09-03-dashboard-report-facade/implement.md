# Implement: dashboard/report façade

1. Confirm `store-query-decouple` and `query-sql-performance` status; rebase if they landed.
2. Route all report SQL through `QueryFilter`.
3. Change loaders to take `&Connection`.
4. Point `blocks_report` at `self.conn`; add open_connection_count assertion.
5. DST test: same since/until on Dashboard trends vs daily report.
6. Run `cargo test --all-features --test cli -- --test-threads=1` and query tests.

Rollback: revert reports.rs + breakdowns.rs + command call sites.
