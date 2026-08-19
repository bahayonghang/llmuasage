# Dash Overview 对齐 tokscale 图表首页

## Goal

让 `llmusage dash` 打开后的 Overview 达到 tokscale Overview 的可读性：先看到按日堆叠的用量趋势，再看到按成本排序的模型名单。

## User Value

用户进 dash 就能判断最近用量从哪几天、哪些模型来，不必先读四张 KPI 卡，再切到 Daily / Models。

## Background

对照来源：用户 2026-08-19 两张截图、`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/overview.rs`、`bar_chart.rs`、`src/tui/panels/overview.rs`、`Dashboard::overview`、已归档任务 `08-18-dash-models-tokscale-style`。完整对照见 `research/tokscale-overview-gap.md`。

当前 Overview 是卡片墙：Total Tokens / 24h Tokens / Total Cost / Cache Hit Rate，加 Token Mix、Recent Activity、Freshness、24h Pulse。tokscale Overview 是 `Tokens per Day` 按模型堆叠柱、最多 5 个图例、可滚动的 `Models by Cost` 双行名单。

`OverviewPayload`（`src/query/mod.rs:82`）只有 lifetime / 24h 合计和新鲜度。`DailyTrendPoint` 没有模型分段。llmusage 没有堆叠柱组件。页脚已经显示 `source · window · tokens · $cost`。`Stats` 和 `/api/overview` 也消费 `OverviewPayload` / `PublicOverviewPayload`。厂商着色已在 Models 任务落地。

## Requirements

- R1. Overview 整页换成 tokscale 构图：上图、中图例、下名单。不再渲染 KPI 卡、Token Mix、Recent Activity、Freshness、24h Pulse。
- R2. 柱图标题为 `Tokens per Day`（很窄时为 `Tokens`）。每个日历日一根柱，按模型堆叠着色。着色键复用 `model_vendor` + `theme::vendor_style` / `vendor_fg`。Y 轴用 `stat_compact`，X 轴标出约 2–3 个日期。
- R3. 图例展示当前窗口成本最高的模型，宽屏最多 5 个、窄屏最多 3 个，格式 `● name`。
- R4. 名单标题默认 `Models by Cost`，右上角 `Total: $x` 为**当前窗口**模型成本合计。每模型两行：`● name (xx.x%)` 和 `In · Out · CR · CW`。窄屏第二行改为 `in/out/cr/cw`。不折叠长尾。
- R5. 柱图和名单跟随现有 TimeWindow 与 source 过滤。`All` 时柱图只画有数据的最近 60 个本地日。页脚 lifetime 合计仍来自未窗口化的 `Dashboard::overview`。
- R6. 名单用现有 `ScrollState` 选择和滚动。默认按 `cost_with_cache_usd` 降序。`o` 在 Tokens 与 Cost 之间循环，`O` 反转方向。标题随排序变为 `Models by Tokens` 或 `Models by Cost`。
- R7. 新增只读查询 `trends_daily_by_model`。不改 `usage_event` / bucket 主键。不改 `OverviewPayload` 既有字段、`PublicOverviewPayload`、`/api/overview`、`/api/home_overview`。
- R8. `NO_COLOR` / `LLMUSAGE_NO_COLOR` / ANSI16 保持 `tui-presentation-contracts.md`。面板和堆叠柱文件不写 `Color::*` 字面量。交互文案保持英文。
- R9. TestBackend 覆盖：新标题与名单文案、空数据、宽/窄第二行格式、滚动只格式化可见行、默认 Cost 降序、NoColor 无前景色。删除对 Token Mix / 24h Pulse 的断言。

## Acceptance Criteria

- [ ] AC1. `llmusage dash` 打开 Overview，主区域是堆叠日柱 + 图例 + 模型名单；缓冲区不含 `Total Tokens`、`Token Mix`、`24h Pulse`、`Freshness`。
- [ ] AC2. 同一厂商族的柱段、图例点和名单名称颜色可区分；与 Models 页同一套厂商映射。`NO_COLOR=1` 下无前景色、无修饰。
- [ ] AC3. 当前窗口内已入库模型都能在名单里滚到；缓冲区不含 `+N more`。
- [ ] AC4. 打开 Overview 时名单按成本降序，标题为 `Models by Cost`。`o` 切到 Tokens 后标题为 `Models by Tokens`。
- [ ] AC5. TimeWindow 为 `All` 时柱数不超过 60；为 `7d` / `30d` / `Today` 时只含该窗口内有数据的本地日。切换窗口会重新加载 Overview。
- [ ] AC6. 页脚仍显示 lifetime tokens 与 `$cost`。`Dashboard::overview` 字段集、`/api/overview` JSON、Stats 使用的 `OverviewPayload` 不变。
- [ ] AC7. `cargo fmt --check`、严格 Clippy、相关 TUI/query 测试通过。

## Out of Scope

- 改 `serve` / ccr-ui `HomeOverviewPayload` 首页。
- 改 Models 宽表、Daily 表、Hourly 表、Cost 折叠、Usage / Stats / Agents / Blocks。
- Overview 的 hourly 粒度切换、Workspace 分组、tokscale 底栏 `Sort: Date Cost Tokens` 重做。
- 重算价目表，或在名单上画 Reasoning / Cache× / Cost/1M / ms/1K。
- 像素级复制 tokscale 主题色。
- 把缺失日期补成零柱。

## Decisions

| 决策 | 选择 | 日期 |
| --- | --- | --- |
| 范围 | 只改 `llmusage dash` Overview，不改 web | 2026-08-19 |
| 对照 | tokscale TUI Overview，不是 DESIGN.md 里的 web profile | 2026-08-19 |
| 构图 | 整页换成图 + 名单，去掉全部现有合计卡 | 2026-08-19 |
| 窗口 | 图和名单跟随 TimeWindow；All 最多 60 根柱；页脚仍是 lifetime | 2026-08-19 |
| 名单形态 | tokscale 双行名单，不复用 Models 宽表 | 2026-08-19 |
