# Dashboard 查询子域模块化

## Goal

保持 `Dashboard` 单连接 facade、所有 root/query re-export 和序列化契约不变，把 `src/query/mod.rs` 中已经独立演化的查询子域迁到有明确 canonical ownership 的垂直模块，使单一功能修改不再要求理解或改动 4,000 行中心生产文件。

## User Value

Dashboard、TUI 与下游库查询行为保持一致，但新增/优化某个分析面板时冲突更少、测试更聚焦，AI/人工维护者能从模块边界直接判断 DTO、SQL 和 helper 的所有者。

## Confirmed Facts

- `src/query/mod.rs` 共 7,137 行，顶层测试区从 4,199 行开始，生产区约 4,198 行。
- 同一 `impl Dashboard` 拥有 overview/trends/breakdowns/context、activity/tools/optimize、compare、health/diagnostics/sync center、snapshot composition。
- explorer、filter、heatmap、home_overview、hour_of_week、logs、top_sessions 已经是独立 query modules，证明 facade + vertical loader 模式是现有惯例。
- `Dashboard::interactive_snapshot` 受 400 ms/128 KiB contract 约束；历史代表性证据已达标，本任务不能用结构移动破坏它。
- `src/web/mod.rs` 的大体量主要是测试，未纳入此重构；`src/query/reports.rs` 由独立 child 处理。

## Requirements

- R1：按稳定领域拆分 overview/timeseries、ranked breakdowns/context、behavior activity/tools/optimize、model comparison、diagnostics/sync center、snapshot composition；不得建立泛化 `queries/common/service` 垃圾抽屉。
- R2：每个 DTO、SQL/helper 和测试只有一个 canonical owner；跨域共享仅限真实 invariant，例如 filter SQL、timezone、scalar helper 或 pricing rollup。
- R3：保留 `Dashboard::open/open_with_busy_timeout`、单一 SQLite connection、interrupt handle、所有方法签名和 `llmusage::query::*`/crate-root public paths。
- R4：所有 JSON field name/default/order assumptions、full/core/interactive snapshot shapes、degraded/cancellation behavior和 filter/timezone 语义逐字段不变。
- R5：现有独立模块不进行顺手重写；`reports.rs`、Store/schema、Web/TUI render 和前端资产不属于本 child。
- R6：测试随所属 feature 迁移，shared facade/serialization/architecture tests 保留在 root 或 integration target；测试发现数量不能下降或重复。
- R7：新增架构 gate 约束 query 不依赖 commands/web/tui，feature module 不反向拥有 facade composition，兼容 re-export 不复制实现。
- R8：结构移动前后 synthetic statement count/payload 完全一致；representative interactive p95/payload/RSS 不回归超过 10%，且继续满足 400 ms/128 KiB。

## Acceptance Criteria

- [x] A1/R1,R2：`query/mod.rs` 生产区只保留 module declarations/re-exports、facade construction 与真正 shared primitives，生产行数不超过 1,200；任一 feature module 生产区不超过 1,000 行且名称能说明所有权。
- [x] A2/R3,R4：public compile fixtures 与 full/core/interactive/section serde snapshots 逐字段相等；Dashboard 仍只打开一个 connection。
- [x] A3/R5：diff 不含 `reports.rs` 查询优化、schema/index、Web/TUI/asset 重构或产品行为变化。
- [x] A4/R6：移动前后单元/integration test leaf 集合相等，除明确新增架构/兼容 fixture；无静默漏跑或重复。
- [x] A5/R7：architecture positive/negative fixtures 捕获 query → commands/web/tui、错误 facade composition ownership 和重复 compatibility implementation。
- [x] A6/R8：固定 synthetic payload/statement count 相等；representative interactive 1d/7d/30d/all 全部 ≤400 ms/≤128 KiB 且 p95/payload/RSS 相对 baseline 不回归超过 10%。
- [x] A7：focused query/web/TUI tests、fmt、严格 clippy、serial tests、rustdoc、Node checks、docs 和 `just ci` 通过。

## Out of Scope

- 优化 SQL、增加索引/缓存/并发、改变 Dashboard API 或 UI。
- 重构 `src/query/reports.rs`、Codex Tracer 或 sync engine。
- 通过把 4,000 行原样搬到一个新 `dashboard.rs` 来满足文件名变化。

## Key Decisions

- 垂直 feature module 拥有 DTO + query + feature-local tests，root facade 通过 re-export 保持兼容。
- 行数门只作为“中心文件不再拥有所有功能”的 guard，不替代所有权和行为验收。
- 先机械移动并保持绿色，再做最小 shared-helper 收敛；不在同一 child 做性能算法优化。
