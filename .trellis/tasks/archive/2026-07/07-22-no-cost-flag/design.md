# --no-cost - 技术设计（C3）

## 边界

- `no_cost` 是 CLI command/output projection；不得加入 `ReportFilter`，也不改变 query、SQLite、
  pricing 或 `TokenTotals`。
- C1 统一报表当前已接受 `render_unified_table(..., no_cost, ...)`，C3 只把共享 args 透传进去。

## 文本

`ReportCommonArgs.no_cost` 被 daily/weekly/monthly/session 命令传给统一渲染器。统一列、数据行、
模型 breakdown 行和 Total 行已经以同一布尔值决定是否 push Cost 单元格，因此不会出现列数不匹配。

未走统一表的既有 surfaces（daily `--instances`、blocks）也必须接收该 flag：其渲染器新增相同的
末列裁剪，保证“所有报表命令”不泄露成本。

## JSON

CLI DTO 先产出正常 camelCase `serde_json::Value`，再在最终输出边界递归移除所有键名含 `cost`
（ASCII case-insensitive）的字段。该投影覆盖 row、`agents`、`modelBreakdowns`、`totals`，并可被
C4 sections 复用。内部 query structs 的 Serialize 不变。

## 验证

- CLI text 测试检查表头和结果不含 `Cost`，但仍含 token/Agent/Total。
- CLI JSON 遍历所有对象，断言无 cost key，且 `totalTokens` 等保留。
- 单测确认 `ReportCommonArgs::to_filter` 未读 `no_cost`，并以现有 totals 回归证明默认输出未变。
