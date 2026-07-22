# weekly 周报命令

## Goal

为 llmusage 新增 `weekly` 报表命令，按周聚合，**接入 C1 的统一 Agent 模型**（默认 Agent 列、
camelCase JSON、`--by-agent`）。周键采用 **周起始日期**（对齐 ccusage），非 ISO 周号。仅新增周期
分支，不改 token 口径/SQLite/同步。

参考：`ref/repo/ccusage/rust/crates/ccusage/src/summary.rs`（`week_start`）+ `adapter/all`。

## Background

- `Commands` 枚举（`src/commands/mod.rs:52`）无 weekly。
- **关键更正**：ccusage 统一周报的 week 键是 `week_start(date, Monday)` 返回的**周起始日期**
  （如 `2025-12-29`），格式 `YYYY-MM-DD`；**不是** `%G-W%V`（如 `2026-W30`）。
- 统一视图固定周一起点；聚焦 `claude weekly` 才有 `--start-of-week`（属 ccusage 扩展，本轮不做）。
- 依赖 C1 已建的 `PeriodKind`/`UnifiedRow`/`load_unified_report`/`render_unified_table`/JSON DTO。

## Requirements

### R1. 命令与参数

- 新增 `Commands::Weekly(WeeklyArgs)`，args 与其它统一报表命令一致（`ReportCommonArgs` + `--by-agent`）。
- 支持全部共享过滤器：`--since/--until`、`--json`、`--breakdown`、`--order`、`--timezone`、
  `--locale`、`--compact`、`--source`、`--by-agent`。接入 `dispatch`；help/命令列表含 `weekly`。

### R2. 周键语义（更正）

- 在 `filter.timezone` 下把事件本地日期归到**所在周的周一日期**，键为该周一的 `YYYY-MM-DD`。
- 统一视图固定周一起点（不暴露 `--start-of-week`）。
- 跨年周正确（12 月末/1 月初的周一归属）。排序遵循 `--order`（默认 desc）。

### R3. 接入 C1 统一模型（非另起渲染）

- `PeriodKind::Weekly` 分支：`load_unified_report(..., Weekly)` 复用 pivot、Agent 列、Detected 标题、
  camelCase JSON DTO、`--by-agent`（给周行加 `agents`）。**不复制** C1 的渲染/JSON 逻辑。
- 默认文本表：逐周 `All`+各来源子行；首列表头 `Week`，单元格显示周起始日期。
- JSON：`{ "weekly": [rows], "totals": {…} }`，行 `period` = 周起始日期。

### R4. 不变式

- 聚合行 = Σ 来源子行（token 精确、cost 1e-9）。
- 同窗口下 weekly `Total` == daily `Total`（跨周聚合不变式）。
- 不改 bucket 加载/成本/同步；不改内部 payload。

### R5. 测试

- 周键单测：Monday 归周、跨年周、时区影响；键为 `YYYY-MM-DD` 周起始日期（**断言非 ISO 周号**）。
- 复用 C1 渲染/JSON 测试范式覆盖 weekly；`Total==daily Total` 不变式。
- 命令解析与 flag 组合测试。

## Acceptance Criteria

- [ ] A1：`llmusage weekly` 默认文本表带 Agent 列（`All`+各来源子行），首列 `Week`=周起始日期，`Total` 行。
- [ ] A2：`weekly --json` = `{ "weekly":[rows], "totals":{…} }`，`period` 为周起始日期（如 `2025-12-29`）。
- [ ] A3：`weekly --by-agent --json` 每周行含 `agents` 数组。
- [ ] A4：周键为周起始日期而非 ISO 周号（针对性断言）；跨年周、周一归周正确。
- [ ] A5：同窗口 weekly `Total` == daily `Total`。
- [ ] A6：`just ci` 全绿；README 双语 + docs 增加 weekly 示例。

## Out Of Scope

- 聚焦 `claude weekly --start-of-week`（可配置周起始日）——ccusage 扩展，本轮不做。
- `--no-cost`（C3）、`--sections`（C4）、聚焦 `<source> weekly`（C5）。

## 依赖

- **依赖 C1（unified-agent-report）先落地**：weekly 直接接入其统一模型/渲染器/JSON DTO。
- 是 C4（weekly 段）与 C5（`<source> weekly`）的前置。
