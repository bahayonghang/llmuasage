# 优化 home_overview 查询性能

## Goal

恢复 `Dashboard::home_overview` 在本地 10k 事件性能门和代表性真实数据库上的稳定查询性能，同时保持 payload、过滤、时区、SQLite 一致性与现有 dashboard 调用契约不变。

## Background

- 父任务 `.trellis/tasks/07-19-claude-sync-scan-performance` 的标准本地完整测试被 `query::tests::home_overview_under_80ms_with_seeded_10k_events` 阻塞。
- 当前实现的 release 隔离测量为 95.69ms；同机 detached 干净 `HEAD` 为 95.77ms，证明它不是父任务引入的回归，但 80ms 本地预算仍真实未满足。debug 隔离测量为 123-154ms。
- `CI` 环境当前使用 500ms 容差，完整测试可通过；本任务不得把 CI 容差当成完成证据，也不得修改 80ms 预算来获得绿灯。
- `src/query/home_overview.rs::load` 依次执行 summary、by-platform、daily series、两条 run-log 查询和 diagnostics。前三段各自聚合 `usage_event`，并包含 session/day distinct；尚无逐阶段计时、VM-step 或 query-plan 证据证明主导项。
- 10k fixture 的 seed 时间不在现有计时区间内；本任务只优化 `home_overview` 查询及为其提供数据的既有 projection/index，不优化测试 seed。

## Requirements

- R1：建立可自动运行、可重复的红灯反馈循环，分别记录 `home_overview` 总耗时和 summary/by-platform/series/run-state/diagnostics 各阶段耗时；至少包含 debug、release、冷 fixture 和一次预热后的结果。
- R2：对主导 SQL 记录 `EXPLAIN QUERY PLAN` 和可比较的 SQLite 工作量证据（VM steps、full-scan/sort 指标或等价结构信号），不能只凭墙钟猜测索引或重写方向。
- R3：优化前后 `HomeOverviewPayload` 必须逐字段等价，包括 session/request/token/cost/cache efficiency、平台默认键、daily series、bootstrap、archive diagnostics、`last_updated`、所有 `QueryFilter` 和时区语义。
- R4：不得放宽、删除、跳过或按平台绕过现有 80ms budget；不得用进程内缓存、预热查询或隐藏 lazy work 让单次冷查询看似通过。
- R5：优先复用 `usage_bucket_30m` 或现有索引/projection；只有查询计划证明既有结构无法满足精确 session/day 语义时，才允许新增 schema migration。新增 projection 必须由 writer/rebuild/repricing 路径一致维护并验证升级成本。
- R6：代表性真实数据库只通过只读连接或在线备份测量，不修改用户数据库。记录事件数、bucket 数、各阶段耗时和最终 wall time，不记录敏感路径、prompt 或事件内容。
- R7：修复范围限于 `home_overview` 查询、直接依赖的 query helper/index/projection、测试基准和对应 spec/ADR；不重构 dashboard API、Web/UI、sync parser 或其他报表。
- R8：标准本地 `cargo test -- --test-threads=1` 必须恢复通过；不能仅以 `CI=1` 或 clean-HEAD 同样失败作为豁免。

## Acceptance Criteria

- [x] A1/R1,R2：同一命令能在修复前稳定捕获 >80ms，并输出足以区分各阶段和主要 SQL 工作量的证据。
- [x] A2/R3：新增 fixture 覆盖跨日重复 session、缺失 session fallback、四平台默认键、source/model/project/date/timezone filter，并证明优化前后 payload 等价。
- [x] A3/R4,R8：现有 `home_overview_under_80ms_with_seeded_10k_events` 在不改阈值的前提下，debug 标准测试和 release 隔离测试各连续通过 3 次。
- [x] A4/R2,R5：主导查询的计划/工作量相对基线显著下降，且没有以新的全表重复扫描、临时 B-tree 放大或 N+1 查询换取墙钟偶然改善。
- [x] A5/R6：真实数据库在线备份上的修复后 wall time 与逐阶段数据已记录，结果不劣于 10k fixture 的方向性结论。
- [x] A6/R5：若新增 schema/projection，fresh、v15 upgrade、idempotent、rebuild 和增量 writer 测试全部通过并记录真实备份迁移耗时；本实现未新增 schema/projection，因此无需 migration。
- [x] A7：`cargo fmt --all -- --check`、严格 Clippy、focused query tests、标准完整串行测试、docs build 与 `git diff --check` 全部通过。
- [x] A8：更新 `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`；本实现未改变 schema 或长期 projection 所有权，因此 ADR 0004 无需更新。

## Out Of Scope

- 放宽性能阈值、仅修改 CI 环境、删除或 ignore 微基准。
- 用 HTTP/前端缓存掩盖 `Dashboard::home_overview` 冷查询成本。
- 修改父任务已完成的 Claude/Codex/OpenCode parser、sync writer 性能方案。
- 无测量依据的全 dashboard/query 层重构，或为了本基准牺牲过滤、时区、session distinct 与 diagnostics 正确性。
