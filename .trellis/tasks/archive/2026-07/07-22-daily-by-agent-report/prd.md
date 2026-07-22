# 统一 Agent 列报表 + camelCase JSON + --by-agent（C1 基础）

> 本任务原名"daily 按 Agent 分组"。经严格 parity 决策（父任务 D0）重定为**基础渲染重构**：
> llmusage 统一报表默认即带 Agent 列，对齐 ccusage `adapter/all`。

## Goal

把 llmusage 的统一报表（`daily`/`monthly`/`session`）改造为 ccusage 的 AllRow 模型：默认文本表带
**Agent 列**（`All` 聚合行 + 各来源子行）、框标题含 `Detected:`；CLI `--json` 改为 ccusage
schema + **camelCase**；`--by-agent` 仅给 JSON 追加 `agents` 数组。为 C2（weekly）预留 period-kind
参数化。**不改** token 统计口径/SQLite/同步，也**不改**内部（dashboard/web/export/TUI）payload。

参考：`ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/{mod,report,types}.rs`（`31e084a`）。

## Background

- 现状 `daily` 默认合并成每天一行、无 Agent 列（`render_daily_summary_table`）；`--json` 为
  snake_case `DailyReport{daily,totals}`。monthly/session 类似。
- ccusage 统一文本表**默认**逐周期出 `All` + 各来源子行；`--by-agent` 只影响 **JSON**（加
  `agents` 数组），session 不加；JSON camelCase `{ "<period>":[rows], "totals":{…} }`。
- 数据层已有 `load_daily_report` + `load_daily_reports_by_source`（`src/query/reports.rs:443`）可
  pivot 出"聚合 + 各来源"。

## Requirements

### R1. 统一行模型与装配（daily/monthly/session）

- 新建统一行结构（含聚合 + `agent_breakdowns` 各来源子行 + `model_breakdowns`），period-kind
  参数化（Date/Month/Session；Weekly 由 C2 接入）。
- 装配复用现有合并 loader + by-source loader 做 pivot，不改底层聚合。
- 不变式：聚合行 token 各字段 = Σ 来源子行（**精确相等**）；`total_cost` = Σ 子行（**1e-9 容差**）。
- `detected` = 窗口内有数据的来源（规范顺序），供文本标题用。

### R2. 默认文本表（带 Agent 列）

- 列（full）：`<Period> | Agent | Models | Input | Output | Cache Create | Cache Read | Total Tokens
  | Cost (USD)`；（compact）：`<Period> | Agent | Models | Input | Output | Cost (USD)`。
- 逐周期：先 `All` 聚合行（Agent=`All`，Models 空或聚合），再各来源子行（Agent=来源
  `display_name`、前缀 `- `，配色 `source_color`）；`--breakdown` 时 model 细分挂在来源子行下。
- `Period` 单元格仅每组首行显示；末尾 `Total` 行（Agent/Models 空）。
- session：行本就按来源，无 `All` 聚合层（对齐 ccusage）。
- 框标题：`Coding (Agent) CLI Usage Report - <Period>` + 次行 `Detected: <display_names>`。
  （标题文案本轮采用 ccusage 文案，属严格 parity。）

### R3. CLI 报表 JSON（camelCase DTO）

- **新建独立 DTO/序列化视图**产出 ccusage schema，**不修改** `TokenTotals`/`DailyReportRow` 等
  内部结构的 Serialize。
- 形状：`{ "<period>": [ row ], "totals": {…} }`，`<period>` ∈ `daily`/`monthly`/`session`。
- 行：`period, agent, modelsUsed, inputTokens, outputTokens, cacheCreationTokens, cacheReadTokens,
  totalTokens, totalCost, modelBreakdowns`。
- `totals`：`inputTokens/outputTokens/cacheCreationTokens/cacheReadTokens/totalTokens/totalCost`
  = 行集合之和。JSON 不含 `detected`。

### R4. `--by-agent`（JSON-only）

- flag 加在统一报表命令 args。仅在 `--json` 下生效：给 daily/monthly 的每行追加
  `agents: [ {agent, modelsUsed, …tokens, totalCost, modelBreakdowns} ]`（各来源）。
- session：`--by-agent` 不改变输出（行本就按来源）。
- 文本表**不因** `--by-agent` 改变（默认已含 Agent 列）。

### R5. 隔离与不变式

- 内部（非 CLI）payload：dashboard/web/export/TUI 使用的结构与 JSON 键**逐字节不变**（P8）。
- 不改 query bucket 加载、成本计算、SQLite、同步。
- period-kind 参数化需让 C2 加 Weekly 时无需复制渲染/JSON 逻辑。

### R6. 测试与迁移

- 聚合单测：`All = Σ agents`（token 精确；cost 1e-9 容差）；`detected` 集合/顺序；`order` 升降序。
- 文本渲染测试：Agent 列、`All`+子行结构、`Period` 仅首行、`Total` 行、compact 列集合、
  `--breakdown` 归属；session 无 `All` 层。
- JSON 测试：camelCase 键、`{<period>:[...],totals}` 形状、`--by-agent` 的 `agents` 数组、
  session 不加 agents。
- **迁移**：更新（而非保留）既有 daily/monthly/session 文本快照与 `--json` 断言；
  证明内部 payload 测试不变（P8）。

## Acceptance Criteria

- [ ] A1：`llmusage daily` 默认文本表含 Agent 列，逐日 `All`+各来源子行，`Date` 仅首行，`Total` 行；
      框标题 `Coding (Agent) CLI Usage Report - Daily` + `Detected: …`。
- [ ] A2：`monthly`/`session` 同样接入统一模型（session 无 `All` 层）。
- [ ] A3：`daily --json` 为 `{ "daily":[rows], "totals":{…} }`、camelCase；`monthly`/`session` 同构。
- [ ] A4：`daily --by-agent --json` 每行含 `agents` 数组；`session --by-agent` 输出不变。
- [ ] A5：聚合不变式测试通过（token 精确、cost 1e-9）；`--breakdown`/`--compact` 正确。
- [ ] A6：内部 dashboard/web/export/TUI payload 测试逐字节不变。
- [ ] A7：`just ci` 全绿；`README.md`+`README.zh-CN.md`+docs 命令示例更新。

## Out Of Scope

- `weekly`（C2 接入统一模型）。
- `--no-cost`（C3）、`--sections`（C4）、聚焦 `<source> <period>`（C5）、`YYYY-MM-DD`（C6）。
- `blocks` 的 Agent 化（ccusage blocks 为 Claude 单源）。
- 新增来源、免 sync、pricing/offline。

## 依赖

- 无前置。**是 C2/C3/C4/C5 的基础**（统一模型、渲染器、JSON DTO）。
