# Dash Stats 对齐 tokscale 年历并优化排版

## Goal

让 `llmusage dash` 的 Stats 达到 tokscale Stats 的可读性：先看到 52 周贡献年历，再看到两列键值摘要。选中一天后打开 Day Breakdown。修掉主次颠倒、日期说明缺年和套娃边框造成的空白浪费。

## User Value

用户打开 Stats 就能扫过一年的活跃分布和连续天数。点一天能看到该日按 source 分组的模型和通道用量。

## Background

对照：用户 2026-08-19 两张截图、`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/stats.rs`、`src/tui/panels/stats.rs`、`src/query/heatmap.rs`。缺口见 `research/tokscale-stats-gap.md`。

同类已归档任务只改各自 dash 页。本任务只改 Stats。

当前页：4 行指标 + 约 10 行 1 列宽 Contribution + `Min` 拉高的 Source Mix + 4 行 Health Signals。`compact_date` 去掉年份后，365 日窗会显示成 `08-20 .. 08-19`。tokscale 是 52 周年历 + 两列 Stats，选中格子后出 Day Breakdown。tokscale 图卡用 `Min` 拉高后内部仍空，不照抄。

## Requirements

- R1. 主区域为 52 周年历，标题 `Contribution Graph (52 weeks)`。格子宽 2（`██`），无数据或窗口前补位用 muted `· `。顶行月份缩写，左侧隔行 weekday（周日为一周第一天，文案 `Mon` / `Wed` / `Fri`）。可见列不够时裁左侧，保留最近周。图卡高度按内容（月份 + 间隔 + 7 行格子 + 边框），不用 `Min` 拉空。
- R2. 底部两列键值 Stats 卡。宽标签：Favorite model、Events、Current streak、Active days、Total tokens、Total cost、Longest streak。窄档（`<80`）改为 Model / Events / Streak / Active / Tokens / Cost / Max streak。Favorite model 取 lifetime `model_breakdown` 中 `cost_with_cache_usd` 最高者，无为 `N/A`。Events 为 `overview.total_events`。Active days 为 `N/M`（heatmap 上 `event_count > 0` 的天数 / 有日期的格子数）。Current streak 与 Longest streak 分开展示。tokens 用 `stat_compact`，成本用 `cost_compact`。
- R3. Stats 卡内另有一行 Context peak / avg（沿用现有 `context_pressure`，仍跟 TimeWindow）。无 priced 事件时写 `n/a`。热力图例在 Stats 卡内：`Less` + 4 档 `██` + `More`。年历底部不再写 `MM-DD .. MM-DD`。
- R4. 页面只有年历卡、Stats 卡，以及选中后的 Day Breakdown。去掉整页外套、Source Mix、Health Signals、`#---` Profile 条。不复制 spending 标语。
- R5. 年历、streak、Favorite model、Events、Total tokens / cost 走 lifetime（`base_filter` + `heatmap(..., 365)`）。TimeWindow 变化不重算年历。source 过滤仍作用在这些 lifetime 查询上。context 继续走 `window_filter`。
- R6. 未选中：年历 + 两列 Stats。选中且高度够：年历 + 紧凑 Stats + `Day Breakdown (ESC to close)`。选中且高度不够：年历 + Breakdown。选中格用 `selection_fill_style`（或等价反色块）。无数据日：`No data for this day`。
- R7. 选格：鼠标点格子；Enter 选中 heatmap 最后一天。Esc 取消选中，不退出 dash。切 tab、切 TimeWindow 取消选中。j/k / Pg / Home / End 在明细打开时滚 Breakdown 行。不改 `h`/`l` 的 TimeWindow 绑定。
- R8. Day Breakdown 首行：本地日期（`%a, %b %d, %Y`）、当日 tokens、当日 cost。其下按 source 分组，组内按 tokens 降序列出 model。宽屏 `In · Out · CR · CW`，窄屏 `in/out/cr/cw`。数据复用当天 `period_model_breakdown`，不新开 SQL。当日 tokens 取 heatmap 该日 `total_tokens`；当日 cost 为明细行 `cost_with_cache_usd` 之和。
- R9. `NO_COLOR` / `LLMUSAGE_NO_COLOR` / ANSI16 保持 `tui-presentation-contracts.md`。面板不写 `Color::*`。交互文案保持英文。Help 补上 Stats 的 Enter / 点击 / Esc。
- R10. TestBackend 覆盖：52 周标题、月份或 weekday、两列标签、`N/A` favorite、Events、`N/M` active days、Context、无 `Source Mix` / `Health Signals` / `#---` / 去年代序说明、空 heatmap、窄标签、Enter 打开当天明细、Esc 关闭且不退出、选中后含 `Day Breakdown`、点击命中日期、NoColor。更新 `tests/tui_panels_prop.rs`。

## Acceptance Criteria

- [ ] AC1. Stats 主区域标题含 `Contribution Graph (52 weeks)`。缓冲区可见月份缩写或 `Mon`/`Wed`/`Fri`。有数据的日子格子占 2 列宽。
- [ ] AC2. 缓冲区含 `Favorite model`（窄档 `Model:`）、`Events`、`Current streak` 或 `Streak:`、`Longest streak` 或 `Max streak:`、`Active days` 或 `Active:`，以及 `N/M` 形式的活跃天数。不含 `Sessions`、`Source Mix`、`Health Signals`、`#---`、`08-20 .. 08-19`。
- [ ] AC3. 缓冲区含 Context 的 peak / avg 或 `n/a`。无 priced 事件时为 `n/a`。
- [ ] AC4. Enter 选中 heatmap 最后一天后，缓冲区含 `Day Breakdown`。有数据日至少出现一个 source 或 model。Esc 后 `Day Breakdown` 消失，进程仍在。
- [ ] AC5. 点击年历上的某一天与 Enter 一样打开该日 Breakdown。点无数据日显示 `No data for this day`。
- [ ] AC6. `NO_COLOR=1` 下无前景色、无修饰。
- [ ] AC7. `/api/heatmap` JSON 字段名和类型不变。`HeatmapPoint` 不加字段。
- [ ] AC8. `cargo fmt --check`、严格 Clippy、相关 TUI/query/data_loader 测试通过。

## Out of Scope

- 改 `serve`、`/api/heatmap`、web 日历热力图。
- 像素级复制 tokscale 蓝主题或 spending 标语。
- 改 Overview / Usage / Models / Daily / Hourly / Monthly / Agents / Blocks 构图。
- 把 heatmap 改成跟 TimeWindow 走。
- 新增 `COUNT(DISTINCT session_id)` 或把 Events 改成 Sessions。
- 用方向键在格子间移动（点另一天或再按 Enter 回今天）。
- 给 `HeatmapPoint` 加 cost。

## Decisions

| 决策 | 选择 | 日期 |
| --- | --- | --- |
| 范围 | 只改 `llmusage dash` Stats，不改 web | 2026-08-19 |
| 对照 | tokscale TUI Stats | 2026-08-19 |
| 热力窗口 | 365 日 lifetime，不跟 TimeWindow | 2026-08-19 |
| 强度 | token 分位 + `theme::heat` | 2026-08-19 |
| Health / Source Mix 卡 | 删除 | 2026-08-19 |
| 图卡高度 | 按年历内容定高 | 2026-08-19 |
| 标语 / 主题蓝 | 不复制 | 2026-08-19 |
| 第三区 | C：格子选中 + Day Breakdown | 2026-08-19 |
| 明细数据 | 复用 `period_model_breakdown` | 2026-08-19 |
| 选格 | 鼠标点格；Enter 选今天；Esc 关闭；不改 `h`/`l` | 2026-08-19 |
| Stats 指标 | A：tokscale 骨架 + Events + Context | 2026-08-19 |
| Sessions | 不做；用 Events | 2026-08-19 |
