# 历史日期 DST 时区正确性（DATA-003）

## Goal

让 `ReportTimezone::Local` 对每个历史日期使用正确的 IANA 时区规则求 offset，消除 DST 地区历史统计的日/周/月归属偏移。

## 覆盖发现（已核实）

- **DATA-003（P2）**：`src/query/filter.rs:11-21` `ReportTimezone::Local` 文档自述"查询时当前 fixed offset 的快照，非 IANA/DST-aware，历史日期复用同一 offset"；`74-81` 行 `local_time_modifier` 用单一 `local_minus_utc()` 秒数生成 SQL 时间修饰符（145-169、266-285 行同源）。测试锁定了此行为。DST 地区冬/夏历史数据边界偏移 1 小时，跨午夜事件进入错误日/周/月，按周期的成本统计失真。

## Requirements

1. 引入 `chrono-tz` 或 `jiff`（或读系统 zone id），`Local` 语义改为按**目标日期**求该日的 UTC offset；`Fixed`/`Utc` 语义保持不变。
2. SQL 侧分组/过滤策略同步调整：单一 offset 修饰符无法表达 DST 切换，需要在 design 中确定按日期段拆分绑定参数或改在 Rust 侧换算边界。
3. 覆盖 spring-forward（不存在的本地时刻）与 fall-back（重复的本地时刻）两类边界；`LocalResult::ambiguous/none` 的处理策略显式化。
4. 更新锁定旧行为的测试与文档。

## Acceptance Criteria

- [ ] DST fixture 矩阵：冬令时/夏令时历史日期各自边界正确（在旧实现上稳定失败）。
- [ ] 跨午夜事件在 DST 切换周归入正确日/周/月。
- [ ] UTC 与 Fixed 时区语义与现有行为完全一致（回归测试）。
- [ ] 无 DST 地区（固定 offset 时区）行为不变。

## Notes

- 审计报告 §1.1 DATA-003；工作量估计 2-5d。
- 涉及 query 层多处 SQL 生成，启动前建议补 design.md 确定 SQL/Rust 边界换算方案。
