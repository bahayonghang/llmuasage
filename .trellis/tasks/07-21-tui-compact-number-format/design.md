# TUI 紧凑数字显示设计

## 边界

本任务只改 `src/tui` 的展示格式和相应测试。输入仍为 query payload 中的 `i64`/`f64`；
formatter 输出 `String`，供 Ratatui cell/Span 使用。任何排序、折叠、选择或计算都继续读取原始字段。

交互 TUI 与非交互 report table 是两个输出契约。`report_table.rs` 继续使用 `grouped` 和
`token_compact`；Web dashboard 继续使用 JavaScript formatter。共享概念不等于共享实现或精度。

## Formatter 契约

在 `src/tui/format.rs` 增加 `stat_compact(i64) -> String`（最终名称可按实现评审微调）：

1. 通过 `unsigned_abs()` 获取安全绝对值。
2. 从 `T/B/M/K` 中选择原值可达到的最高量级。
3. 将缩放值四舍五入到一位小数；若结果达到 `1000.0`，提升到下一量级后重新格式化。
4. 输出最多一位小数，移除 `.0`；负值恢复 `-`。
5. 小于 1,000 的值直接输出整数。

必须通过边界测试固定舍入行为，避免浮点实现细节在量级边缘产生 `1000K`。若直接使用 `f64`
难以稳定满足边界，应使用整数商/余数计算十分位，而不是放宽测试。

## 调用点迁移

| 表面 | 紧凑显示 | 保持精确/原样 |
| --- | --- | --- |
| Overview | token、events、avg/event、buckets | cost、percent、timestamps |
| Footer | total tokens | cost |
| Models | token、event count、collapsed token total | cost、model |
| Daily / Hourly | token、event count | date/hour、cost |
| Cost | token、event count | USD cost、source/model |
| Stats | token、event count | streak/day labels、percent |
| Behavior | token、turn/call/session counts | USD、ratios、scores |
| Blocks | token、burn rate、projected token | time labels、percent/cost |
| Usage sync center | 无默认迁移 | 对账所需的扫描/插入/存储/跳过计数 |

迁移完成后，若 `tokens` 或 `footer_compact` 已无调用，可删除这些因本任务而过时的 helper；
`axis_compact`、`grouped`、`token_compact` 等未被本任务替代的契约不做顺手清理。

## 测试策略

- `src/tui/format.rs`：表驱动 formatter 单测，覆盖 0、999、1K、K/M/B/T 边界、舍入进位、
  负值、`i64::MAX`、`i64::MIN`。
- `tests/tui_panels_prop.rs`：使用公开的生产 formatter 构造预期值，删除测试内复制的旧千分位
  实现；为 Overview 加入与截图同量级的固定回归数据。
- 表格面板：至少为每类迁移字段提供一个 >= 1B 或 >= 1M 的代表值，验证可见文本；既有排序、
  windowing、selection 测试继续证明行为未改变。
- report table：保留现有 `978.05K`、`5.37M`、`40.33B` 断言，证明非交互输出未被波及。

## 兼容与回滚

这是纯展示变更，无数据迁移和配置兼容问题。回滚只需恢复 formatter 调用点及契约文档；数据库和
查询结果不受影响。实现应作为一个原子 TUI 变更提交，避免出现部分面板新格式、部分面板旧格式的
中间状态。

## 后缀决策

使用大写 `K/M/B/T`，与 Web 和 report table 的后缀一致。该决定只约束交互 TUI 新 formatter，
不得改变现有 report-table helper 的两位小数契约。
