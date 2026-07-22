# --since/--until 支持 YYYY-MM-DD

## Goal

让报表日期过滤器 `--since/--until` 接受 `YYYY-MM-DD`（与 ccusage 一致，如 `--since 2026-04-25`），
同时**保留现有 `YYYYMMDD` 兼容**。仅改日期解析，不动过滤/聚合逻辑。

## Background

- ccusage：`ccusage daily --since 2026-04-25 --until 2026-05-16`（连字符格式）。
- llmusage 现要求 8 位纯数字 `YYYYMMDD`：`parse_date_value`/`parse_report_date`
  （`src/commands/report_args.rs:196`）会拒绝含连字符的输入。
- 该解析器由所有报表命令的 `ReportCommonArgs` 共用，一处修改全局生效。

## Requirements

### R1. 解析

- `--since/--until` 同时接受：
  - `YYYYMMDD`（8 位数字，现有格式）；
  - `YYYY-MM-DD`（ISO 连字符格式）。
- 两种格式解析到同一 `NaiveDate`；非法值（错误位数、非法月日、其他分隔符）给出清晰错误，
  错误信息更新为同时说明两种可接受格式。
- `value_parser` 的回显值与内部 `parse` 保持一致（`parse_report_date` 与 `parse_date_value` 同步）。
- 更新 arg 的 `value_name`（现为 `YYYYMMDD`）为体现两种格式，如 `YYYY-MM-DD|YYYYMMDD`，使 `--help` 准确。

### R2. 范围与不变式

- 不改过滤窗口语义、`--all` 交互、排序或聚合。
- 所有报表命令（daily/weekly/monthly/session/blocks）自动获得新格式支持。
- 现有 `YYYYMMDD` 用法与既有测试保持通过。

### R3. 测试

- 表驱动解析测试：`20260425` 与 `2026-04-25` 解析为同一日期；`2026/04/25`、`2026-4-5`（若不接受）、
  `abcd` 等按契约拒绝。
- 端到端/命令级：`daily --since 2026-04-25 --until 2026-05-16` 正常运行（至少一条命令级测试佐证
  透传到实际过滤）。

### R4. 文档

- 更新 `README.md` + `README.zh-CN.md` + `docs/` 中日期过滤示例，标注两种可接受格式。

## Acceptance Criteria

- [ ] A1：`--since 2026-04-25` 与 `--since 20260425` 行为一致。
- [ ] A2：两种格式对所有报表命令生效。
- [ ] A3：非法输入报错并说明两种可接受格式；既有 `YYYYMMDD` 测试不变。
- [ ] A4：`--help` 的 value-name 体现两种格式；命令级测试佐证过滤生效。
- [ ] A5：`just ci` 全绿；README 双语 + docs 日期示例更新。

## Out Of Scope

- 相对日期（`today`、`-7d` 等）或其他日期语法。
- 时区/周边界改动（各自属于其它任务）。

## 依赖

- 无。独立小改动，可随时并入。
