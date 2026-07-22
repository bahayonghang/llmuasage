# 统一 Agent 列报表 — 技术设计（C1）

参考：`ref/repo/ccusage/rust/crates/ccusage/src/adapter/all/{mod,report,types}.rs`（`31e084a`）。

## 边界

- 只重构**统一报表的装配 + 呈现 + CLI JSON**；不改 `load_filtered_buckets`/`Aggregate`/SQLite/
  token accounting/成本/同步。
- **不改**内部结构（`TokenTotals`/`DailyReportRow`/…）的 `#[derive(Serialize)]`；这些继续供
  dashboard/web/export/TUI 使用，键名/形状不变（P8 红线）。CLI JSON 走**独立 DTO**。

## 统一行模型

新建（建议 `src/query/reports.rs` 或新模块 `src/query/unified.rs`）：

```rust
pub struct UnifiedRow {
    pub period: String,                     // Date / Month / Session-id（Weekly 由 C2）
    pub agent: UnifiedAgent,                // All | Source(SourceKind)
    pub tokens: TokenTotals,                // 复用内部 totals（仅作数值容器）
    pub models_used: Vec<String>,
    pub agent_breakdowns: Vec<UnifiedRow>,  // 各来源子行；session 为空
    pub model_breakdowns: Vec<ModelCostBreakdown>,
}
pub struct UnifiedReport { pub kind: PeriodKind, pub rows: Vec<UnifiedRow>, pub detected: Vec<SourceKind> }
pub fn load_unified_report(store, filter, kind: PeriodKind) -> Result<UnifiedReport>
```

装配（方案 A，复用已测 loader）：`load_daily_report`/`load_monthly_report`/`load_session_report`
给聚合行；`load_daily_reports_by_source`（及 monthly 对应）给 `agent_breakdowns`，按周期键 pivot。
session 天然按来源 → 每行一个来源，无 `All` 层。

不变式（测试固定）：聚合行 token 各字段 == Σ `agent_breakdowns` token（精确）；`total_cost` 用
1e-9 容差（`token-accounting-contracts.md`）。

## 文本渲染

`src/tui/report_table.rs` 新增 period-kind 参数化的 `render_unified_table(report, compact,
no_cost, color_mode)`（mirror ccusage `print_table`）：

- 列：full = `<Period>|Agent|Models|Input|Output|Cache Create|Cache Read|Total Tokens|Cost`；
  compact = `<Period>|Agent|Models|Input|Output|Cost`。`no_cost` 由 C3 引入（本任务先按 cost 常在
  实现，预留 `no_cost` 形参/列裁剪点）。
- 逐 `UnifiedRow`：push 聚合行（Agent=`All`）；再逐 `agent_breakdowns` push 来源子行（Agent=
  `descriptor_for(kind).display_name`，前缀 `- `，`source_color`）；`--breakdown` 时其 model 细分
  紧随。`Period` 仅每组首行。末尾 `Total` 行。
- 标题：`print_box_title` 等价——`Coding (Agent) CLI Usage Report - <Period>\nDetected:
  <display_names>`。

## CLI JSON DTO（camelCase）

新增独立序列化视图（`serde` 手写或专用 struct，字段 `rename_all="camelCase"`）：

- `report_json(kind, rows, include_agents)` → `{ "<rows_key>": [row_json...], "totals": totals_json }`。
- `row_json`：`period, agent, modelsUsed, inputTokens, outputTokens, cacheCreationTokens,
  cacheReadTokens, totalTokens, totalCost, modelBreakdowns`；`include_agents` 时加
  `agents:[agent_json...]`（仅 daily/monthly）。
- `totals_json`：六字段之和。`json_float` 处理 cost。**不含 `detected`**。
- 命令层：`daily.rs`/`monthly.rs`/`session.rs` 的 `--json` 分支改用该 DTO；`--by-agent` 传
  `include_agents`。

## 命令接线

- `report_args.rs`：统一报表命令 args 增加 `--by-agent`（`-A`，JSON-only 语义）。移除旧的
  daily-only Open Decision。
- `daily.rs`/`monthly.rs`/`session.rs`：默认走 `render_unified_table`（文本）与新 DTO（JSON）；
  daily 的 `--instances`（按项目）分支保留其现有行为。
- session 命令：`--by-agent` 解析但对输出无效（文档说明）。

## 测试策略

- `src/query`：`load_unified_report` 单测（pivot 正确、不变式、detected、order、session 无 All 层）。
- `src/tui/report_table.rs`：`render_unified_table` 快照（Agent 列、All+子行、Period 首行、Total、
  compact 列集合、breakdown 归属）。
- CLI JSON：camelCase/形状/`--by-agent` agents/session 不加。
- 迁移：**改写**既有 daily/monthly/session 文本与 JSON 断言为新契约；新增 P8 断言证明内部 payload
  不变（可对比 dashboard snapshot 测试保持通过）。

## 兼容与回滚

- CLI 文本/JSON 有意破坏性；内部 payload 经 DTO 隔离不变。原子提交，避免默认表新旧混合。
- 回滚 = 撤销本任务提交（统一模型、渲染器、DTO、命令接线、迁移测试）。C1 是 C2–C5 前提。

## 为 C2/C3/C4 预留

- `PeriodKind` 含 `Weekly`（C2 只补装配的周键与该分支）。
- `render_unified_table`/DTO 已带 `no_cost` 形参裁剪点（C3 填充 strip 逻辑）。
- DTO 的 `report_json`/`totals_json` 可被 C4 sections 复用（扁平多段 + 命令段 totals）。
