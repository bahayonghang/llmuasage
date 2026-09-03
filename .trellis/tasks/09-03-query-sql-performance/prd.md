# 查询层全表加载与 N+1 改为 SQL 聚合

## Goal

Dashboard / CLI 读路径用 SQL 聚合或带过滤查询，不再把匹配的全部 `usage_event` 拉进进程，也不再按源/host 发 N+1 的 `MAX(event_at)`。

## Background

交互预算见 `.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`。当前实现：

- PERF-001 `activity.rs:75-85` 无过滤加载全部 event cost；`legacy_activity_breakdown` 已有 SQL join 但仅测试。
- PERF-002 `home_overview.rs:336-372` 全量事件进 `Vec`。
- PERF-003 `tools.rs` 物化 tool 行 + 过滤后的 event 再在 Rust 分摊成本。
- PERF-004 `top_sessions.rs:223-293` 生产路径投影全部事件；`load_legacy` 已有 GROUP BY + LIMIT。
- PERF-005 `reports.rs` session / `--id` 用 `visit_filtered_events` 全扫。
- PERF-006 `breakdowns.rs:310-370` 每个 source/host 一次 `MAX(event_at)`。
- PERF-007 `tui/data_loader.rs:307-323` 未选 source 时按注册源各打一次 `context_pressure`。
- PERF-008 `overview.rs` 同一 bucket 过滤重复跑约 8 次查询。

## Requirements

- R1. `activity_breakdown` 的成本来自过滤后的 SQL join/lookup，不扫描全表 `usage_event`。
- R2. `home_overview` compact/full 用 SQL 聚合（`COUNT(DISTINCT)` 会话、按日/源求和），不把全部事件载入 `Vec`。
- R3. tool attribution 在 SQL 侧 join/聚合，或至少只加载当前 filter 需要的列。
- R4. `top_sessions` 在 tokens/cost 排序下用 GROUP BY + LIMIT；duration 排序若仍需事件时间，必须有上限或二次查询，不得无 LIMIT 投影全表。
- R5. session 报表与 `--id` 把 `session_id` 推进 SQL。
- R6. source/host breakdown 的 `last_event_at` 一次分组查询。
- R7. TUI 未选 source 时一次 `context_pressure`（已按 source, model 分组）。
- R8. overview 合并可合并的 bucket 扫描，语义与现有 JSON 字段一致。
- R9. `HOME_PLATFORMS` 硬编码四源（`home_overview.rs:14`）改为注册源或显式“主平台”列表，不得默默丢掉 pi/omp/grok/kimi_code。若产品要保留四源卡片，PRD 实现前在 design 写明并更新文案。
- R10. 现有 10k 事件 facade 性能测试不得回退；新增至少一条“不得 `SELECT` 无 WHERE 的 usage_event 全表”的回归（activity）。

## Acceptance Criteria

- [ ] AC1. `activity_breakdown` 的 SQL 含 filter 或 join，不再是无 WHERE 的 `FROM usage_event`。
- [ ] AC2. home overview 不再构造按事件的全量 `Vec<HomeOverviewEvent>`。
- [ ] AC3. top_sessions tokens/cost 路径 SQL 含 GROUP BY 与 LIMIT。
- [ ] AC4. `load_single_session_report` / session `--id` SQL 含 session 谓词。
- [ ] AC5. source/host breakdown 对 N 个组只发常数次查询（1 次 MAX 分组，不是 N 次）。
- [ ] AC6. TUI 全源 context_pressure 只调用一次 Dashboard 方法。
- [ ] AC7. 现有 query 集成测试与 `facade_performance` 通过；payload 字段名与数值语义不变。

## Out of scope

- 拆 `reports.rs` 文件体积（`09-03-dashboard-report-facade`）。
- 改 live interactive snapshot 的 section 集合。
- 导出 HTML 仍可走完整 snapshot；但内部查询必须用本任务的聚合，不得再全表扫。

## Ordering

不依赖其他子任务。与 `dashboard-report-facade` 都改 `reports.rs` 时，本任务先合入。
