# --sections - 技术设计（C4）

## 参数模型

新增 `UnifiedReportArgs`，由 daily/weekly/monthly/session flatten：

```text
--by-agent
--sections daily,weekly,monthly,session
```

`ReportSectionArg` 使用 Clap `ValueEnum` 和 `value_delimiter=','`，非法值由 Clap 列出合法段。
blocks 不 flatten 这组参数，因而不能伪装成 sections host。

## 段序与加载

`requested_sections(command_kind, requested)` 返回：

```text
[command_kind] ++ [Daily, Weekly, Monthly, Session 中被请求且不等于 command_kind 的项]
```

因此当前段始终存在、优先且没有重复。每段均调用 C1/C2 的 `load_unified_report`。

没有显式日期范围时，daily 段（无论宿主命令是什么）在段级 filter 上补最近七日；`daily --all`
禁止该补充。其他段保持现有全历史默认。显式 `--since/--until` 原样复制给所有段。

## 输出

- 文本：按结果顺序调用 `render_unified_table`，段间插入一个空行。
- JSON：共享 DTO 暴露 `sections_json`，把每个段的 rows 平铺为 `{kind:[rows]}`，最后写入单个
  `totals`（宿主段 `UnifiedReport::totals()`）。自定义 `SerializeMap` 保持字段顺序，不依赖
  `serde_json::Map` 的实现细节。
- `--by-agent` 对非 session 段传入 rows DTO；session 强制不加 `agents`。`--no-cost` 在最终字段
  values 上递归 strip，复用 C3 helper。

## 保持的单命令行为

没有 `--sections` 时，命令仍走当前单段路径和 daily 的 `--all`/默认窗口规则；sections 只扩展
多段代码路径。

## 验证

- 单测 `requested_sections` 的固定顺序、去重和当前优先。
- 集成测试 raw JSON 文本的字段顺序、宿主 totals、by-agent/no-cost 透传和错误 parser。
- 文本测试验证 title 顺序，且 single command snapshot 继续通过。
