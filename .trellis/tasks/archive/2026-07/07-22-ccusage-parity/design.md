# ccusage parity — 跨子任务技术设计（父任务）

本设计只锁定**跨子任务的共享决策与边界**；各子任务在自身 design.md 细化实现。参考实现位于
`ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/`（`31e084a`）。

## 统一报表模型（AllRow）

对齐 ccusage `adapter/all`：统一报表走**单一 period-kind 参数化**路径，供 daily/weekly/monthly/
session 复用。核心行结构（llmusage 侧新建，勿复用现有 report 结构直接改序列化）：

```
UnifiedRow {
    period: String,            // date / week-start-date / month / session-id
    agent: 聚合行=All 标签，子行=来源
    tokens: input/output/cache_creation/cache_read/total,
    total_cost: f64,
    models_used: Vec<String>,
    agent_breakdowns: Option<Vec<UnifiedRow>>,   // 各来源子行（文本恒展开；JSON 仅 --by-agent）
    model_breakdowns: Vec<...>,                   // --breakdown
}
```

- 数据装配复用现有 `load_daily_report` + `load_daily_reports_by_source`（`src/query/reports.rs`）做
  pivot，得到"聚合 + 各来源 breakdown"；不改底层 bucket/聚合语义。
- 聚合不变式：聚合行 token 各字段 = Σ 来源子行（精确相等）；`total_cost` = Σ 子行成本（**1e-9 容差**，
  见 `token-accounting-contracts.md`）。

## 文本渲染（默认即 Agent 列）

对齐 ccusage `print_table`：逐周期行先 push 聚合行（Agent=`All`），若有 `agent_breakdowns` 再逐个
push 来源子行（缩进/前缀 `- `）；`--breakdown` 下把 model breakdown 挂在对应来源子行之下。
列（full）：`<Period> | Agent | Models | Input | Output | Cache Create | Cache Read | Total Tokens
| Cost`；（compact）：`<Period> | Agent | Models | Input | Output | Cost`。`--no-cost` 移除末列。
末尾 `Total` 行（Agent/Models 空）。框标题：`Coding (Agent) CLI Usage Report - <Period>` +
第二行 `Detected: <来源 display_name 列表>`。session 无 `All` 聚合层，行本就按来源。

## CLI 报表 JSON（camelCase DTO，隔离内部 payload）

- **新建独立 DTO/序列化视图**产出 ccusage schema，**禁止**修改 `TokenTotals`/`DailyReportRow` 等
  内部结构的 `#[derive(Serialize)]`（那些仍供 dashboard/web/export/TUI 使用，键名不变）。
- 形状：`{ "<period>": [ row ], "totals": { … } }`，`rows_key` ∈ daily/weekly/monthly/session。
- 行：`period, agent, modelsUsed, inputTokens, outputTokens, cacheCreationTokens, cacheReadTokens,
  totalTokens, totalCost, modelBreakdowns`；`--by-agent` 追加 `agents: [ {同结构各来源} ]`
  （daily/weekly/monthly；session 不加）。
- `totals` 为对应行集合之和；`--sections` 的 `totals` 取**命令自身段**的行集合。

## week 键（C2）

`week_start(date, Monday)` 语义：本地日期归到所在周的**周一日期**，格式 `YYYY-MM-DD`。统一视图固定
周一起点；聚焦 `claude weekly --start-of-week` 属 ccusage 扩展，本轮不做（若做归 C5/后续）。

## `--sections`（C4）

`requested = [command_kind] ++ [D,W,M,S 中被请求且非 command_kind 者]`（当前段必含、去重、固定周期
序、当前优先）。文本逐段 `print_table`；JSON 扁平 `{ "<段>":[rows]…, "totals": 命令段 totals }`
（有序 map）。

## `--no-cost`（C3）

呈现/序列化投影，**不进 query filter**：
- 文本：列集合省略 Cost 列（含表头、数据、Total）。
- JSON：对最终 Value 执行 cost-strip（对齐 ccusage `strip_cost_json`），移除所有层级的
  `totalCost`/成本字段（含 `agents`、`modelBreakdowns`、`totals`）。
- 覆盖全部报表结构；token 字段与非成本字段不受影响。

## 聚焦能力矩阵（C5）

`<source> <period>`：单来源、**无 Agent 列**（对齐 focused 视图）。真实能力（`cli-commands.json`）：
Claude=daily/weekly/monthly/session/blocks(+statusline)；Codex=daily/monthly/session；
OpenCode=daily/weekly/monthly/session；Antigravity 按其解析器实际可得周期定。**非**均匀 4×5。
llmusage 若选择比 ccusage 更宽的均匀支持，须在 C5 明确标注为 llmusage 扩展而非 parity。

## 共享参数

`--no-cost`、`YYYY-MM-DD` 解析加在 `ReportCommonArgs`；`--by-agent` 加在统一报表命令 args（
daily/weekly/monthly/session）。聚焦子命令复用同一 args 但强制单 source 且隐藏 Agent 列。

## 兼容与回滚

- CLI 文本/JSON 为**有意破坏性**变更；内部 payload 不变（DTO 隔离）。
- 每个子任务应为原子提交，避免默认表新旧混合状态。
- 回滚粒度到子任务；C1 是基础，回滚 C1 会连带影响 C2–C5 的前提。
