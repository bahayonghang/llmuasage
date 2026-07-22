# weekly 周报 - 技术设计（C2）

## 边界

- 只新增 `weekly` 命令和 `PeriodKind::Weekly` 的 query 分支；不改 token accounting、SQLite
  schema、sync 或内部 dashboard/export/TUI payload。
- 复用 C1 的 `UnifiedReport`、`render_unified_table` 与 CLI JSON DTO，不复制文本布局或
  camelCase 序列化逻辑。

## 周键

`week_start(date)` 返回输入本地日期所在周的周一：

```text
week_start = date - date.weekday().num_days_from_monday()
```

结果以 `%Y-%m-%d` 格式作为 period key。该计算发生在已有 `ReportFilter.timezone` 转换之后，因而
UTC 边界事件先归属本地日期，再归属周一。键是周起始日期，不使用 ISO 年/周号。

## Query 接线

`load_unified_report(..., PeriodKind::Weekly)` 使用与 C1 daily/monthly 相同的 aggregate +
per-source pivot：

- aggregate loader 按 week-start group buckets；带 project filter 时沿用 event-detail 路径；
- per-source loader 采用同一键函数，产出 `UnifiedRow::agent_breakdowns`；
- `UnifiedReport` 仍从 aggregate rows 取排序，来源行保持 descriptor 稳定顺序；
- `UnifiedReport::totals()` 直接加总周聚合行，因此和同窗口 daily totals 相同。

## Command 接线

新增 `WeeklyArgs`（`ReportCommonArgs` + `--by-agent`）和 `Commands::Weekly`。命令层调用：

```text
load_unified_report(..., Weekly)
  -> render_unified_table(...)       // human output
  -> unified_report::report_json(...) // --json
```

`--by-agent` 仅控制 JSON `agents`，与 daily/monthly 行为一致。`render_unified_table` 从
`PeriodKind::first_column()` 得到 `Week`，避免 weekly 专用渲染分支。

## 验证

- 单测周一、跨年和时区边界的 week-start key，明确断言没有 `%G-W%V`。
- 断言 weekly 的 `All = sum(agents)` 和 weekly totals 等于 daily totals。
- 集成测试 CLI JSON/`--by-agent`/human table；最后进入父任务全量 `just ci`。
