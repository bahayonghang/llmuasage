# tokscale Daily / Hourly / Monthly vs llmusage dash

对照日期：2026-08-19

对照来源：用户五张截图、`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/{daily,hourly,monthly,hourly_profile,footer,widgets}.rs`、`src/tui/panels/{daily,hourly}.rs`、`Dashboard::trends` / `trends_daily`、已归档 `08-18-dash-models-tokscale-style` 与 `08-19-dash-overview-tokscale-style`。

## tokscale 三页

| 页 | 宽表列 | 交互 |
| --- | --- | --- |
| Daily | Date, Turn, Msgs, Input, Output, Cache R, Cache W, Cache×, Total, Cost, Cost/1M | Date/Cost/Tokens 排序；今日行高亮；Enter 进入当日 Model×Source 明细；Esc 返回 |
| Hourly | Hour, Source, Turn, Msgs, Input, Output, Cache R, Cache W, Cache×, Total, Cost, Cost/1M | 默认表；`v` 切 Hourly Profile；日期分组行 `%m/%d`；小时只显示 `%H:00`；当前小时高亮 |
| Monthly | Month, Turn, Msgs, Input, Output, Cache R, Cache W, Cache×, Total, Cost, Cost/1M | 与 Daily 同排序；Enter 进入该月 Daily Breakdown |

窄档：very narrow = 时间 + Cost；narrow = 时间 + Turn/Msgs + Tokens + Cost。Turn 列在全部 `turn_count == 0` 时整列隐藏。

Cache× = `cache_read / (input + cache_write)`，一位小数带 `x`；分母 0 且 read > 0 为 `∞`，都为 0 为 `—`。Cost/1M = `cost / total * 1e6`。

数据：`DailyUsage` / `HourlyUsage` / `MonthlyUsage` 自带 token 四通道、cost、`message_count`、`turn_count`。Hourly 还有 `clients: BTreeSet<String>`。Hourly 是整点小时，不是 30 分钟。

tab 顺序：Overview, Usage, Models, Daily, Hourly, Minutely, Monthly, Sessions, Stats, Agents。

## 当前 llmusage

tab（恰好 9 个，数字键 1–9）：Overview, Usage, Models, Daily, Hourly, Cost, Stats, Agents, Blocks。`Panel::COUNT = 9`（`src/tui/app.rs:16-32`）。没有 Monthly 面板。没有 Minutely。

### Daily（`src/tui/panels/daily.rs`）

- 数据：`Dashboard::trends_daily` → `DailyTrendPoint`（`src/query/mod.rs:140-157`）：date、input、cache_read、cache_creation、output、total、event_count、cost。
- 宽列：Date, Events, Input, Output, Cache R, Cache W, Cache%, Total, Cost。
- Cache% = `cache_read / (input + cache_read + cache_creation) * 100`，与 Models / tokscale 的 Cache× 不同。
- 无 Cost/1M、Turn、Msgs。底部 2 行 detail，无 Enter 明细页。
- 默认 `items.iter().rev()` 即日期降序。`o/O` 已支持 Date / Tokens / Cost。
- 跟随 TimeWindow。`/api/trends_daily` 直接序列化 `DailyTrendPoint`。

### Hourly（`src/tui/panels/hourly.rs`）

- 数据：`Dashboard::trends("hourly", …)` → `TrendPoint`（`src/query/mod.rs:109-114`）：只有 `label` + `total_tokens`。注释写明该函数留给 `/api/trends?window=`（`src/query/mod.rs:1033-1037`）。
- 列：Hour, Tokens, Share, Profile（`#`/`-` 条）。无 Input/Output/Cache/Cost/Source。
- `hour_start` 是 30 分钟桶（截图有 `03:30`）。Share 相对当前窗口全部桶；`window All` 时近期小时都是 `0.0%`。
- 无排序。无日期分组行。

### Monthly

- TUI 无此页。
- CLI 已有 `load_monthly_report`（`src/query/reports.rs:771`）和 `MonthlyReportRow`，走 `usage_bucket_30m` 按本地月聚合。桶路径下 `conversation_count` 恒为 0（`add_bucket` 不记会话）。

## 可复用

- `tui::format::{cache_multiplier, cost_per_million, stat_compact, cost_compact}` 与 Models 合同一致。
- `theme::{metric_input, metric_output, metric_cache_read, metric_cache_write, metric_cache_hit, metric_cost_per_million, positive_fg}`。
- `ScrollState` / `SortState` / `stable_sort_refs`。Daily 已挂 Date/Tokens/Cost。
- `trends_daily_by_model`：Overview 用的按日×模型 token。Enter 明细若要 Cost/Source 需加字段或新查询。
- `usage_bucket_30m` 有 source、model、四通道、event_count、cost、`hour_start`。Hourly Source 可用 `GROUP_CONCAT(DISTINCT source)`（与 Models 相同）。
- `usage_turn.started_at`（`src/store/migrations.rs:719-738`）可按本地日/时/月 `COUNT(*)` 得到 Turn。全 0 则按 tokscale 隐藏 Turn 列。
- Msgs 最近似值是桶上的 `event_count`（当前 Daily 的 Events）。

## 缺口

1. Daily Cache% 与 Cache× 公式不一致；缺 Cost/1M。
2. Hourly 载荷只有 total_tokens，无法画 tokscale 宽表。
3. Hourly 是 30 分钟点，tokscale 是整点小时 + 日期分隔行。
4. 没有 Monthly 面板；加第 10 个 tab 会碰到 1–9 数字键和 “nine panels” 合同（`tui-presentation-contracts.md`）。
5. Turn 不在桶上，要另查 `usage_turn`。
6. Daily 只有 footer detail，没有 Enter 日明细；Monthly 没有日下钻。
7. Hourly 没有 tokscale 那种按时段/星期聚合的 Profile 页（当前条形图不是同一视图）。
8. `TrendPoint` / `/api/trends` / `DailyTrendPoint` 公共 JSON 被 web 使用，不能改字段集。

## 已拍板（2026-08-19）

- Monthly 插在 Hourly 后，数字键 `6`。
- 删除 TUI Cost tab，保持 9 个面板。
- `/api/costs` 与 `cost_breakdown` 保留。

## 不在本对照内

- tokscale Minutely、Sessions。
- 像素级主题色。
- web `serve` 的 trends 图。
- CLI `llmusage daily|monthly` 报表文本。
