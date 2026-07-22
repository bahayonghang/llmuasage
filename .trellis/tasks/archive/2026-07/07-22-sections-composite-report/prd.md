# --sections 组合报表

## Goal

新增 `--sections <list>`：一次调用输出多个周期段，严格对齐 ccusage 语义（当前段必含、去重、固定周期
序、当前优先）与 JSON schema（扁平多段 + 命令段 totals）。承接父任务 D1'——`--all` 保持"全历史"，
组合能力由 `--sections` 承担。

参考：`ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/{mod.rs(requested_sections),
report.rs(sections_report_json)}`（`31e084a`）。

## Background

- ccusage：任一统一命令（daily/weekly/monthly/session）皆接受 `--sections`。
- **更正**：语义不是"按输入顺序输出"，而是：
  - `requested = [command_kind] ++ [Daily,Weekly,Monthly,Session 中被请求且非 command_kind 者]`；
  - **当前命令段必含**、**去重**、**固定周期序**、**当前段优先**。
- **更正**：JSON 不是嵌套完整报表，而是**扁平** `{ "<段>":[rows]…, "totals": 命令段 totals }`
  （有序 map，单个 `totals` 取命令自身段行集合之和）。

## Requirements

### R1. 参数与宿主

- 在所有统一报表命令（daily/weekly/monthly/session）加 `--sections <csv>`，合法段
  `daily|weekly|monthly|session`；非法段名报错并列合法值。
- 出现 `--sections` 时，命令输出组合报表（仍包含命令自身段）。

### R2. 段集合语义（更正）

- 最终段序 = 当前命令段在前，其余按固定周期序（Daily,Weekly,Monthly,Session）补入被请求者；去重。
- 例：`monthly --sections daily,session` → 段序 `[monthly, daily, session]`（monthly 必含且优先）。

### R3. 过滤器透传

- `--since/--until/--timezone/--locale/--source/--order/--breakdown/--compact/--no-cost/--by-agent`
  对每段一致生效（`--by-agent` 仅影响各段 JSON 的 agents；`--no-cost` 对文本列与 JSON strip）。
- 各段沿用自身默认窗口，除非被 `--since/--until` 覆盖。

### R4. 文本输出

- 按 R2 段序逐段 `render_unified_table`（各段含 Agent 列 + 自己的框标题）；段间清晰分隔。

### R5. JSON 输出（更正）

- 扁平有序 map：`{ "<段1>":[rows], "<段2>":[rows], …, "totals": {…} }`。
- `totals` = **命令自身段**行集合之和（camelCase 六字段）；**不是**各段合计，也不是每段各带 totals。
- 各段 rows 复用 C1 的 camelCase 行 DTO；`--by-agent` 时各行含 `agents`（session 段除外）。

### R6. 不变式与范围

- 复用 C1/C2 的 `load_unified_report` 与 DTO，不新增聚合语义；本任务是编排层。
- 单命令（无 `--sections`）输出不受影响。

### R7. 测试

- 段序单测：当前段必含/优先、去重、固定周期序（多组 command_kind × sections 输入）。
- 文本测试：多段标题与表格按段序出现。
- JSON 测试：键集合==段集合、顺序==段序、`totals` 来自命令段、`--by-agent`/`--no-cost` 透传。

## Acceptance Criteria

- [ ] A1：`daily --sections daily,monthly,session` 文本按 `[daily,monthly,session]` 逐段输出。
- [ ] A2：`monthly --sections daily,session --json` → 键序 `[monthly,daily,session,totals]`，
      `totals` 为 monthly 段之和（**含当前段、带 totals**）。
- [ ] A3：非法段名报错并列合法值；去重生效。
- [ ] A4：过滤器（`--since`、`--by-agent`、`--no-cost`）对每段一致生效。
- [ ] A5：单命令输出不受影响；`just ci` 全绿；README 双语+docs 更新。

## Out Of Scope

- 复用/改动 `--all`（保持全历史）。
- `blocks` 作为段（ccusage sections 不含 blocks）。
- 段级独立窗口的复杂定制（超出 `--since/--until`）。

## 依赖

- **依赖 C1（统一 DTO/渲染）与 C2（weekly 段）**。若在 C2 前实现，`weekly` 段暂不可选并在校验中拒绝，
  待 C2 落地后启用（实现时定）。`--no-cost`/`--by-agent` 段级透传随 C3/C1 落地情况。
