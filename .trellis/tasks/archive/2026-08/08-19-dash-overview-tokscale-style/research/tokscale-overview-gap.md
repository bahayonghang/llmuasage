# tokscale Overview vs llmusage dash Overview

对照日期：2026-08-19

## tokscale Overview

源码：`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/overview.rs`、`bar_chart.rs`。

| 区域   | 行为                                                                                                      |
| ------ | --------------------------------------------------------------------------------------------------------- |
| 布局   | 柱图约 35% 高度（最少 5 行）+ 1 行图例 + 剩余给名单                                                       |
| 柱图   | `Tokens per Day`；按日堆叠；段色来自模型厂商色；Y 轴顶部一个最大值，底部 `0`；X 轴 2–3 个日期；8 档块字符 |
| 数据窗 | daily 取最近 60 条后反转为时间升序；hourly 代码存在但截图是 Daily                                         |
| 图例   | 宽 5 / 窄 3；`● name`；按当前排序的模型列表截取                                                           |
| 名单   | `Models by Cost` 或 `Models by Tokens`；右上 `Total: $x`；每模型两行；可滚动；选中反色                    |
| 第二行 | 宽：`In · Out · CR · CW`；窄：`in/out/cr/cw`                                                              |
| 占比   | `cost / sum(cost) * 100`，一位小数                                                                        |

## 当前 llmusage Overview

源码：`src/tui/panels/overview.rs`、`Dashboard::overview`。

- 四张 KPI 卡 + Token Mix / Recent Activity / Freshness + 24h Pulse。
- `OverviewPayload` 无按日×模型序列。
- `DailyTrendPoint` 只有按日合计。
- Overview 不走 TimeWindow，也不用 `ScrollState`。
- 页脚用 `overview.total` 显示 lifetime tokens / cost。
- `/api/overview` 投影 `PublicOverviewPayload`，字段必须保持。

## 可复用

- `model_breakdown`：名单的 In/Out/CR/CW/cost。
- `model_vendor` + `theme::vendor_style`。
- `stat_compact` / `cost_compact`。
- `ScrollState` / `SortState` / `selection_fill_style`。
- Stats 面板的「一次请求跑多条查询」模式。

## 缺口

1. 没有 `GROUP BY local_date, model` 查询。
2. 没有堆叠柱组件。
3. Overview 测试锁死了 Token Mix / 24h Pulse 文案。
4. `panel_uses_time_window` 不含 Overview。
