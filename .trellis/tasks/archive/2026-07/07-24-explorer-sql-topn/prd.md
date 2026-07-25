# Explorer SQL Top-N 与查询预算（PERF-001）

## Goal

把 Explorer 的 Top-N 从 Rust presentation 层下推到 SQL query plan，使 DB 物化行数与 Rust 内存随 `limit × buckets` 而非全量数据增长，并为查询建立硬预算。

## 覆盖发现（已核实）

- **PERF-001（P1）**：`src/query/explorer.rs:265-332`：`load()` 先全量收集 `all_rows`（290-295 行），求和后才 `select_rows(&all_rows, query.limit, ...)`（297 行）截断；series 同样全量 `load_*_series` 后 `collapse_series`（301-310 行）。SQL 对所有 group/series `GROUP BY` 并排序，无 SQL `LIMIT`（553-627、1402-1482 行的 SQL 生成）。高基数 session/tool/project × 时间 bucket 导致 DB 临时表、CPU、内存随全量数据增长；GET 请求可长期占用 query permits。

## Requirements

1. rows：CTE 先 aggregate，再 `ORDER BY value DESC LIMIT ?` 在 SQL 中做 Top-N。
2. series：先确定 top key set；top key 正常 aggregate；非 top key 在 SQL 中按 bucket 汇成一条 `Other`，不把每个 tail group × bucket 传回 Rust。
3. 硬预算：最大查询天数、最大时间 bucket 数、最大 materialized points（默认 ≤5,000，硬上限 ≤20,000）、最大 response bytes；超预算返回明确 422/413 而非超时 500。
4. 为每个 dimension/metric 声明 required table、cardinality class、index requirements、max range、degrade behavior（审计 §3.3.1 方向，可在本任务内先做核心部分）。

## Acceptance Criteria

- [ ] query instrumentation 证明 DB 物化 rows ≤ `limit+1`，series 点数 ≤ point budget。
- [ ] `EXPLAIN QUERY PLAN` 无不必要的全表 temp sort，关键 index 命中。
- [ ] Rust peak RSS 不随全量 group 数线性增长。
- [ ] 合成 benchmark（10 万 session、100 万 event、365 天 daily）p95 < 2s（阈值按实际硬件校准后写入测试）。
- [ ] 超预算请求返回 422/413 与稳定 error code。
- [ ] 现有 Explorer API 响应结构（rows/series/Other/totals）语义不回归，前端无需改动或同步改动。

## Notes

- 审计报告 §1.2 PERF-001 深挖含目标 SQL 草图与复杂度分析；§3.3.1、§6.4。
- 复杂任务：启动前需补 design.md（query plan 设计）+ implement.md。
