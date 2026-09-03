# 切断 store 对 query 的循环依赖

## Goal

`src/store` 不再 import `crate::query`。定价计算与 SQLite timezone 函数成为 query 之下的依赖，而不是反向依赖。架构测试锁定这条边。

## Background

`store/mod.rs:14-18` 使用 `query::pricing`。`store/connection.rs:57-58` 调用 `query::timezone::register_functions`。`sync_writer.rs`、`pricing_catalog.rs`、`migrations.rs` 同样碰到定价类型。`query` 仍拥有 `Store`。架构测试 `tests/architecture/main.rs` 不扫描 `store`→`query`。

## Requirements

- R1. `src/store/**` 无 `crate::query` 路径（use、全限定、别名）。
- R2. 定价类型与 `compute_cost_with` 的所有权移到 store 可依赖的模块（建议 `src/domain/` 或 `src/store/pricing.rs` 抽出的 crate 内模块）。query 改为 re-export 或改用新路径，对外 `llmusage::query` 定价 API 保持兼容或按 `library-api.md` 更新。
- R3. SQLite 本地日期/小时函数注册从 `query::timezone` 迁到 store 可调用的位置；query 报表仍用同一套函数名。
- R4. `tests/architecture` 增加 `store_does_not_depend_on_query`，覆盖 use / 全限定 / 别名（与现有 ARCH-002 扫描同级）。
- R5. 成本重算、catalog apply、sync commit 的数值语义不变。

## Acceptance Criteria

- [ ] AC1. 对 `src/store` 跑与 `tests/architecture` 相同的依赖扫描，0 条 `crate::query`。
- [ ] AC2. 新架构测试失败用例夹具证明扫描能抓到 `store`→`query`。
- [ ] AC3. 现有 store/query/sync 定价与 DST 测试通过。
- [ ] AC4. 不在本任务统一 `ReportFilter`（那是下一子任务）。

## Out of scope

- 拆 `reports.rs` / 合并 Dashboard 连接。
- 缩小 `lib.rs` 兼容模块的 pub 面。

## Ordering

先于 `09-03-dashboard-report-facade` 合入。
