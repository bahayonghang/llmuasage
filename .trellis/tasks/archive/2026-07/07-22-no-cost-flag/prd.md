# --no-cost 隐藏成本

## Goal

新增 `--no-cost`：文本表省略成本列、JSON strip 掉所有成本字段。**纯呈现/序列化投影，不进 query
filter**（对齐 ccusage）。加在共享 `ReportCommonArgs`，一次生效于所有报表命令与统一 Agent 表。

参考：`ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/report.rs`（`no_cost`→`strip_cost_json`
与 total_row `pop`）。

## Background

- ccusage：`no_cost` 在 SharedArgs，传给打印器；文本 `total_row.pop()` 去 cost 列，JSON 走
  `strip_cost_json` 递归剥离成本字段。**不参与聚合/查询**。
- **更正（Codex #7）**：llmusage 不应把 `no_cost` 放进 `ReportFilter`（`src/query/reports.rs:23`
  是查询层）。应在**渲染器列集合**与 **CLI JSON DTO 输出**两处投影。
- 成本出现在：C1 统一表 `Cost (USD)` 列（含聚合行/来源子行/Total）、CLI JSON `totalCost`
  （行、`agents`、`modelBreakdowns`、`totals` 各层级）。

## Requirements

### R1. 参数落点（更正）

- `ReportCommonArgs` 新增 `--no-cost`（布尔），经**命令层**传给渲染器与 JSON DTO；**不进
  `ReportFilter`**，不影响装配/聚合。
- 默认 false；语义为"隐藏"，不改任何底层成本数值。

### R2. 文本投影

- 统一表在 `--no-cost` 下从列集合移除 `Cost (USD)`（表头、聚合行、来源子行、`Total` 行同步）。
- full 与 compact 两套列集合都需去成本列；其余列宽自适应。
- 聚焦视图（C5，若已落地）同样去成本列。

### R3. JSON 投影（strip）

- 对最终 camelCase Value 递归剥离成本：行 `totalCost`、`agents[].totalCost`、
  `modelBreakdowns[].*cost*`、`totals.totalCost` 全部移除（对齐 `strip_cost_json`），而非置 0。
- `--sections`（C4，若已落地）各段与 totals 一并 strip。

### R4. 范围与不变式

- token 与非成本字段完全不受影响；不改成本计算/pricing。
- 与 `--breakdown`/`--compact`/`--by-agent`/`--json`/`--sections` 组合一致隐藏成本。

### R5. 测试

- 文本：daily + 至少一个其它周期，`--no-cost` 下无 Cost 列、token 列完整（含 Agent 子行、Total）。
- JSON：`--no-cost --json` 递归无 `totalCost`（含 `agents`、`modelBreakdowns`、`totals`）。
- 回归：不带 `--no-cost` 的文本/JSON 断言不变。

## Acceptance Criteria

- [ ] A1：`daily --no-cost` 文本表无 `Cost (USD)` 列（聚合行/来源子行/Total 均无），token 列完整。
- [ ] A2：`--no-cost` 对 weekly/monthly/session 一致（含 compact 列集合）。
- [ ] A3：`--no-cost --json` 递归剥离所有 `totalCost`/成本字段，非置 0。
- [ ] A4：`no_cost` 未进入 `ReportFilter`/查询层（代码审查 + 测试佐证）。
- [ ] A5：不带 flag 输出不变；`just ci` 全绿；README 双语+docs 更新。

## Out Of Scope

- 成本量级/币种改造；改成本计算或 pricing；`--offline`。

## 依赖

- **依赖 C1（统一渲染器 + JSON DTO）**；建议在 C1、（尽量）C2 之后实现，使投影一次覆盖所有周期。
  与聚焦视图（C5）在成本列上交叠：谁后落地谁补对方的隐藏成本测试。
