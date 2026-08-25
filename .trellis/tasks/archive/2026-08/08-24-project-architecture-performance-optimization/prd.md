# 项目架构与性能债务优化

## Goal

在不改变 CLI、公共 Rust facade、SQLite 数据语义和 Dashboard/TUI wire contract 的前提下，分阶段收敛当前项目中已经由代码证据确认的四类架构与性能债务，使来源摄取、同步应用层、查询子域和报表聚合各自拥有清晰、可测量、可回滚的边界。

## User Value

- 大规模本地历史不再让 Codex Tracer 摄取随总事件数无界占用内存。
- 同步能力可以作为稳定库接口复用，而不依赖 CLI 命令模块拥有核心执行逻辑。
- Dashboard 查询变化可以局限在所属子域，降低跨 4,000 行中心文件修改的回归面。
- `--project` 报表保留现有模糊匹配语义，同时避免统一报表的重复逐事件扫描。

## Confirmed Facts

- `src/commands/codex_tracer/` 约 8,415 行，拥有独立 parser、schema、store、server 和前端；独立 `codex-tracer.db` 是已记录产品决策，不能在本任务中合并进主数据库。
- Tracer 当前按文件返回 `Vec<CodexTracerEvent>`，命令层再把所有文件事件累积到 `all_events` 后一次写入；parser 使用 `BufRead::lines()`，而主 Codex parser 已有 4 MiB 有界 JSONL reader、durable offset 和取消语义。
- 稳定根 facade 导出 `JobRegistry`，但它的 `Default`、具体 executor 和主要同步编排位于 `src/commands/sync.rs`。
- `src/query/mod.rs` 的测试区从第 4,199 行开始；生产区同时拥有概览、趋势、行为、比较、诊断、同步中心和快照等多个子域。
- `src/query/reports.rs` 的日/周/月、source/host 组合存在平行聚合代码；`load_unified_report` 分别加载总计和按来源结果，项目模糊过滤则在 SQL 之后逐事件匹配。
- 本轮安全合成验证通过：architecture 3/3、100k backlog 报表回归 1/1、10k home overview 1/1。详细边界见 `research/baseline.md`。

## Task Map

| Child | Priority | Deliverable |
| --- | --- | --- |
| `08-24-codex-tracer-bounded-ingestion` | P1 | 共享有界 Codex 记录边界、流式批量写入、durable file state、规模基准 |
| `08-24-sync-application-boundary` | P1 | sync 应用核心拥有 executor/default/编排，commands 仅保留 CLI I/O 适配 |
| `08-24-dashboard-query-modularization` | P2 | `query/mod.rs` 按垂直子域拆分，公共 facade 与性能契约不变 |
| `08-24-report-project-filter-performance` | P1 | 一次周期聚合管线、项目维度解析、bucket 快路径和代表性基准 |

父任务不直接实现产品代码，只拥有源需求、任务映射、跨子任务兼容性与最终集成门。

## Requirements

- R1：每个子任务先冻结行为等价 fixture 和性能/工作量基线，再实施结构或查询变化；不得用无测量的“更快”作为验收。
- R2：保留 `llmusage codex-tracer`、独立 tracer 数据库、主 `llmusage.db`、CLI 参数/JSON、Dashboard/TUI payload 与 root Rust facade 的兼容性。
- R3：共享机制必须有单一 canonical owner；兼容旧公共路径时使用 re-export/薄 adapter，不复制第二套实现。
- R4：架构改造不得绕过 ADR 0002 的 shard commit、ADR 0003 的 Store facade、写 fencing、token accounting、source sync 和 report CLI 契约。
- R5：性能验收必须同时覆盖结果等价、SQLite query plan/statement count 或 bounded-memory 结构证据，以及墙钟/RSS；只改善一个指标不能掩盖其他回归。
- R6：不得读取或修改用户活动数据库做普通测试；代表性数据只能来自明确的只读源加临时备份，证据不得记录 prompt、原始事件、用户路径或标识值。
- R7：四个子任务独立提交、验证和归档；涉及同一公共 re-export/架构测试时由后执行子任务基于当前 HEAD 调整，不能回退先前子任务。
- R8：父级最终集成必须在四个子任务完成后重新运行公共 API、架构、报表、sync、Dashboard/TUI 相关门和 `just ci`，并汇总性能证据的适用范围。

## Acceptance Criteria

- [x] A1/R1：四个子任务均留存 before/after 证据并标注证据级别；dashboard 代表性基准于 2026-08-25 补齐（见其 `research/representative-performance.md`），RSS/cold-cache 保持显式 `UNVERIFIED`。
- [x] A2/R2,R3：公共 API fixture 3/3、CLI JSON/report/tui 兼容测试随子任务通过；兼容路径仅 re-export，canonical ownership 由架构 fixture 约束。
- [x] A3/R4：source sync、write fencing、token accounting、report CLI、Dashboard performance focused gates 在各子任务内全绿。
- [x] A4/R5：report 子任务 100k backlog 回归与 dashboard 交互矩阵（p95 80–91 ms / ≤73 KiB）达标；结构子任务无 p95/payload 回归。
- [x] A5/R6：性能证据仅含白名单计数、耗时、payload 摘要；真实库访问使用只读临时副本并在用后删除。
- [x] A6/R7：四个子任务均完成、提交并归档；本 task map 与实际 commit 一致。
- [x] A7/R8：`just ci` 于最终产品树 `5c6fddc` 通过；归档后复核 architecture 10/10、api facade 3/3、query lib 48 通过。

## Out of Scope

- 合并或移除 Codex Tracer 独立数据库/专用 UI。
- UI 视觉重设计、新来源接入、token/cost 语义变化、远端协议变化。
- 无基准支持的全库索引堆叠、异步并发扩容或缓存掩盖。
- 处理本任务创建前的历史 Trellis 归档质量问题。

## Key Decisions

- 保留 Codex Tracer 的独立产品边界，只下沉共享的 bounded record/decode primitive。
- 优先级顺序为 Tracer bounded ingestion → sync boundary → report performance → query modularization；后两项可在文件不重叠时并行，但父级集成串行。
- 结构优化默认行为等价；任何 wire/schema/产品语义变化都必须回到 planning 重新评审。
