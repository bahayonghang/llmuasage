# Dash Daily / Hourly / Monthly 对齐 tokscale 周期表

## Goal

让 `llmusage dash` 的 Daily、Hourly、Monthly 达到 tokscale 周期表的可读性：按日/时/月看到 Turn、Msgs、四通道 token、Cache×、Cost、Cost/1M。Hourly 按本地整点小时和日期分组行展示。tab 条为 Overview / Usage / Models / Daily / Hourly / Monthly / Stats / Agents / Blocks。

## User Value

用户在 dash 里就能按时间粒度比较成本和缓存倍率，不必再开 tokscale。Hourly 在 `window All` 下也不再全是 `0.0%` 空条。

## Background

对照来源：用户 2026-08-19 五张截图、`ref/repo/tokscale` 的 Daily/Hourly/Monthly TUI、`src/tui/panels/{daily,hourly,cost}.rs`、`Dashboard::trends` / `trends_daily`。完整对照见 `research/tokscale-period-gap.md`。

同类已归档任务：`08-18-dash-models-tokscale-style`、`08-19-dash-overview-tokscale-style`、`08-19-dash-usage-tokscale-style`。本任务只改周期页和 tab 条，不改 Overview / Usage / Models 构图。

当前 Daily 已有四通道和 Cost，但 Cache 画成百分比，无 Cost/1M，底部 2 行 detail。Hourly 走遗留 `TrendPoint`（label + total_tokens），30 分钟桶，Share 相对窗口合计。第 6 个 tab 是 Cost（source×model 折叠表）。没有 Monthly。

`usage_bucket_30m` 能填 Input/Output/Cache/Total/Cost/Msgs(`event_count`)/Hourly Source。Turn 来自 `usage_turn.started_at`。Cache× / Cost/1M 格式器已在 Models 落地。`/api/trends`、`/api/trends_daily`、`/api/costs` 的公共 JSON 不能改字段集。

## Requirements

- R1. Daily 宽表列：Date, Turn, Msgs, Input, Output, Cache R, Cache W, Cache×, Total, Cost, Cost/1M。Msgs = `event_count`。Turn 来自该日 `usage_turn` 计数；窗口内全部为 0 时隐藏 Turn 列。不再渲染 `Cache%`、`Events`、底部 2 行 detail。
- R2. Hourly 默认改为 tokscale 表。宽表列：Hour, Source, Turn, Msgs, Input, Output, Cache R, Cache W, Cache×, Total, Cost, Cost/1M。底层仍读 `usage_bucket_30m`，按本地整点小时聚合。日期用 `%m/%d` 分组行，小时只显示 `%H:00`。Source 为该小时去重后按 id 排序的 source 列表。不再渲染 Tokens/Share/Profile 条。
- R3. Monthly 是独立面板，接在 Hourly 后面。宽表列：Month, Turn, Msgs, Input, Output, Cache R, Cache W, Cache×, Total, Cost, Cost/1M。Month 为本地 `YYYY-MM`。
- R4. 去掉 TUI Cost tab。`Panel` 仍为 9 项，顺序：Overview, Usage, Models, Daily, Hourly, Monthly, Stats, Agents, Blocks。数字键 `6` 打开 Monthly。不改 `Dashboard::cost_breakdown`、`/api/costs`、CLI 报表。
- R5. 窄档：`<60` 为时间 + Cost；`<80` 为时间 + Turn/Msgs + Tokens + Cost。宽表挤不下时 Date 掉年（`MM-DD`），Month 保持 `YYYY-MM`。
- R6. Cache× 与 Cost/1M 复用 `cache_multiplier` / `cost_per_million`。通道色走 metric 槽，Cost 走 positive，Cache× / Cost/1M 走已有语义槽。面板不写 `Color::*`。
- R7. 三页都跟随现有 TimeWindow 与 source 过滤。`o` 在 Date / Tokens / Cost 间循环，`O` 反转方向。打开时默认 Date/Month 降序。
- R8. Daily 按 Enter 进入当日明细（Model、Source、Msgs、四通道、Cache×、Total、Cost）。Monthly 按 Enter 进入该月 Daily Breakdown，列与 Daily 宽表相同。Esc 从明细返回列表；不在明细时 Esc 仍退出 dash。`q` 始终退出。切 tab 时关闭明细。Hourly 不做 Enter。
- R9. 新增 TUI 用的小时/月查询和日/月明细查询。不改 `TrendPoint`、`/api/trends`、`/api/trends_daily` 既有字段、CLI `MonthlyReport` JSON。`DailyTrendPoint` 若加 TUI 字段必须 `serde(default, skip_serializing)`。不改 `usage_event` / bucket 主键。
- R10. `NO_COLOR` / `LLMUSAGE_NO_COLOR` / ANSI16 保持 `tui-presentation-contracts.md`。交互文案保持英文。Help 去掉 Cost、写上 Monthly 与 Enter/Esc。
- R11. TestBackend 覆盖：三页宽/窄列、Hourly 日期分隔与 `%H:00`、Cache× 非 `%`、Cost/1M、默认 Date 降序、Enter/Esc、Turn 全 0 隐藏、导航含 Monthly 不含 Cost、滚动只格式化可见行、NoColor。删除 Hourly Share/Profile 条和 Cost 面板的断言。

## Acceptance Criteria

- [ ] AC1. Daily 宽表表头含 Turn（有数据时）、Msgs、Cache×、Cost/1M；缓冲区不含 `Cache%`、`Events`、`detail ` 前缀行。
- [ ] AC2. Hourly 宽表表头含 Hour、Source、Msgs、Input、Output、Cache R、Cache W、Cache×、Total、Cost、Cost/1M。缓冲区不含 `Share`、`Profile`、`#---` 条。同一天的行上方有 `%m/%d` 分组行，小时列为 `HH:00`。
- [ ] AC3. `window All` 时 Hourly 仍能按成本和四通道读最近小时；不再出现整列 `0.0%`。
- [ ] AC4. tab 条为 `1 Overview` … `5 Hourly` `6 Monthly` `7 Stats` `8 Agents` `9 Blocks`。缓冲区不含独立 `Cost` tab 标签。`6` 打开 Monthly。
- [ ] AC5. Monthly 行按本地月聚合，宽列与 Daily 同构（时间列换成 Month）。
- [ ] AC6. Daily / Monthly 按 Enter 进入明细、Esc 返回列表。明细行能滚到该日已入库模型或该月日期。不在明细时 Esc 仍退出。
- [ ] AC7. 三页默认 Date/Month 降序。`o` 切到 Cost 后对应列表头带 `▼`。
- [ ] AC8. `NO_COLOR=1` 下无前景色、无修饰。
- [ ] AC9. `/api/trends`、`/api/trends_daily`、`/api/costs` JSON 字段名和类型不变。CLI monthly 报表 JSON 不变。
- [ ] AC10. `cargo fmt --check`、严格 Clippy、相关 TUI/query 测试通过。

## Out of Scope

- 改 `serve` / ccr-ui 的 trends 图或 `/api/trends*` / `/api/costs`。
- tokscale Minutely、Sessions、Hourly Profile（按时段/星期聚合的 `v` 页）。
- 像素级复制 tokscale 主题色。
- 改 Overview / Usage / Models / Stats / Agents / Blocks 的现有构图。
- 重算价目表。伪造 ms/1K。
- 把 CLI `llmusage monthly` 文本表换成 TUI 渲染。
- 把 `Panel::Sources` / `Panel::Projects` 重命名为 Daily / Hourly（只改显示名和 Cost→Monthly）。

## Decisions

| 决策 | 选择 | 日期 |
| --- | --- | --- |
| 范围 | 只改 `llmusage dash` 周期页和 tab 条，不改 web | 2026-08-19 |
| 对照 | tokscale TUI Daily/Hourly/Monthly | 2026-08-19 |
| Monthly 位置 | Hourly 后，数字键 `6` | 2026-08-19 |
| Cost tab | 从 TUI 删除；查询与 `/api/costs` 保留 | 2026-08-19 |
| 面板数 | 仍为 9 | 2026-08-19 |
| Hourly 粒度 | 展示整点小时；底层仍读 30 分钟桶 | 2026-08-19 |
| Hourly 默认 | 表，不保留 Share/Profile 条 | 2026-08-19 |
| Msgs | `event_count` | 2026-08-19 |
| Turn | `usage_turn` 计数；全 0 隐藏列 | 2026-08-19 |
| 默认排序 | Date/Month 降序 | 2026-08-19 |
| Cache× / Cost/1M | 复用 Models 格式器 | 2026-08-19 |
| Daily footer | 删除，改 Enter 明细 | 2026-08-19 |
| Hourly Profile `v` | 不做 | 2026-08-19 |
