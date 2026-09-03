# 统一 Dashboard 与 CLI reports 读路径

## Goal

CLI 报表与 Dashboard 共用同一套过滤器和同一条 SQLite 连接语义，避免 DST/host/source 过滤漂移，也避免 TUI `blocks_report` 再开第二条连接。

## Background

`Dashboard` 持有一条 `Connection`（`query/mod.rs:85-90`）。`query::reports::ReportFilter` 自己 `open_connection`（`reports.rs`）。`Dashboard::blocks_report`（`breakdowns.rs:261-279`）构造 `ReportFilter` 后调用 `load_blocks_report(&self.store, ...)`，注释“snapshot 只开一次库”不成立。`QueryFilter::sql_filter` 与 `push_event_filter` 各写一套 source/host/date。

依赖：若 `09-03-store-query-decouple` 未合入且同时改 `src/query/`，先合入 decouple。本任务不要求 store 已与 query 解耦才能开始设计，但实现顺序写在 implement.md。

## Requirements

- R1. 读侧只有一个过滤器类型承载 source/model/since/until/host/timezone；报表特有字段（order、locale、breakdown、blocks 选项）作为附加结构，而不是第二套 SQL 生成器。
- R2. `load_*_report` 接受已打开的 `Connection`（或 `&Dashboard`），不再为每次报表 `Store::open_connection`。
- R3. `Dashboard::blocks_report` 使用 `self.conn`。
- R4. CLI daily/weekly/monthly/session/blocks JSON 字段与现有契约一致（`.trellis/spec/llmusage/backend/report-cli-contracts.md`）。
- R5. 架构测试：禁止 `query::reports` 在非测试代码里 `store.open_connection`（或等价断言 Dashboard 路径共享连接）。

## Acceptance Criteria

- [ ] AC1. `blocks_report` 不增加 `open_connection` 计数（可用现有 test counter）。
- [ ] AC2. 同一 `QueryFilter` 下 Dashboard overview 与 CLI daily 的日期边界一致（DST 用例）。
- [ ] AC3. 现有 cli/query report 测试通过。
- [ ] AC4. 不在本任务做 SQL 全表扫描改造（`09-03-query-sql-performance`）。

## Out of scope

- 把 `report_table` 移出 `tui`。
- 合并 daily/weekly/monthly 命令文件（可在 hygiene 顺手，非 AC）。
