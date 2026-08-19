# Design: Dash Overview 对齐 tokscale 图表首页

## Architecture

本任务停在 TUI Overview 展示层和一条只读查询。`Dashboard::overview`、web 投影、解析器、store 主键、定价目录都不改字段。

```
usage_bucket_30m
    ├ Dashboard::overview(&filter)                 # lifetime 合计，页脚 / Stats / /api/overview
    ├ Dashboard::trends_daily_by_model(&window)    # 新增：local_date × model 的 total_tokens
    └ Dashboard::model_breakdown(&window)          # 现有：名单通道 + 成本
         → OverviewPanelPayload                    # TUI-only
              → stacked_bar + legend + two-line list
```

## Boundaries

| 层             | 做                                                                        | 不做                                                         |
| -------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------ |
| query          | 新增 `DailyModelPoint` 与 `trends_daily_by_model`                         | 改 `OverviewPayload` 字段、改 `trends_daily` 形状            |
| web            | 无                                                                        | 改 `/api/overview`、`PublicOverviewPayload`、`home_overview` |
| TUI state      | `AppState.overview` 改为 `OverviewPanelPayload`；Overview 纳入 TimeWindow | 改其他面板 payload                                           |
| theme          | 增加 `vendor_fg(vendor, rank) -> Color`，走 `adapt_color`                 | 面板或柱图文件写 `Color::*`                                  |
| Overview 面板  | 重写成图 + 图例 + 双行名单                                                | 保留 KPI / Token Mix / 24h Pulse                             |
| Models / Daily | 不改                                                                      | 把宽表搬进 Overview                                          |

## Data flow

1. Overview 的 panel 请求仿 Stats：一次 loader 任务里串行跑三条只读查询。
   - `overview(&filter)`：未窗口化，给页脚 lifetime。
   - `trends_daily_by_model(&window_filter)`：柱图。
   - `model_breakdown(&window_filter)`：名单和图例。
2. `panel_uses_time_window` 与 `invalidate_windowed_panel_data` 加上 `Panel::Overview`。切窗口会清掉并重载 Overview。
3. `All` 时查询仍可返回全部有数据的日；渲染层按日期升序后只取最后 60 个本地日。更短窗口不补零日。
4. 名单在内存里按 `SortState` 排。默认 `cost_desc()`。图例取排序后的前 5 / 3 个。
5. `update_scroll_total` 对 Overview 使用模型行数。可见行 = 名单内高 / 2。

## Contracts

### Query

```text
DailyModelPoint { date: YYYY-MM-DD, model: String, total_tokens: i64 }
GROUP BY local_date, model
ORDER BY local_date ASC, model ASC
timezone = QueryFilter::local_date_expr("hour_start")
```

不进 `PublicOverviewPayload`。不序列化到 `/api/*`。

### TUI payload

```text
OverviewPanelPayload {
    totals: OverviewPayload,          # 未窗口化
    daily_models: Vec<DailyModelPoint>,
    models: Vec<ModelBreakdown>,
}
```

页脚继续读 `totals.total.total_tokens` 与 `totals.total_cost_usd`。

### Chart

- 标题：`Tokens per Day`；`area.width < 60` 时 `Tokens`。
- 高度：`floor(area.height * 0.35).max(5)`，再留 1 行图例。
- 柱宽按可用宽度均分。段色 = `vendor_fg(vendor_from_model(model), shade_rank)`。
- shade 表用**完整** `models` payload 建一次，滚动和换日不变色。
- 同一天内按模型名稳定排序后再堆叠，避免帧间闪色。
- Y 轴只在顶行写 `stat_compact(max)`，底行写 `0`。
- X 轴约 3 个标签（很窄 2 个），把 `YYYY-MM-DD` 收成 `Mon D`。

### List

- 标题随 `SortState`：Cost → `Models by Cost`；Tokens → `Models by Tokens`；很窄 → `Top Models`。
- 右上角 `Total: {cost_compact(sum)}`，很窄只写金额。
- 占比分母是窗口内有限成本之和，至少按 `0.01` 托底，避免除零。
- 选中行用 `selection_fill_style`，名称保留厂商色。
- 空数据：`No model data found.`；柱图无日：不画柱，不崩。

### Runtime

- Overview 加入可滚动面板集合。
- `sort_keys(Overview) = [Tokens, Cost]`。
- `AppState::new`：`sort[Overview] = cost_desc()`，与 Models 一致。
- 可见行只格式化 `visible_range`。

## Compatibility

- `Dashboard::overview` 单测和 `/api/overview` 断言保持。
- Stats 仍自己调 `overview()`，不读 `OverviewPanelPayload`。
- 改 `AppState.overview` 类型后，更新 footer、draw、loader、相关 AppState 测试。

## Trade-offs

- 一次 Overview 请求三条查询，而不是把日×模型塞进 `OverviewPayload`。这样 web JSON 不变，Stats 也不被迫拉图数据。
- `All` 封顶 60 日是展示上限，不是查询硬删。窗口内数据仍完整供给名单。
- 不把 Models 宽表复用到 Overview：截图要的是双行扫描名单，宽表已经在 Models 页。

## Rollback

无 migration。还原 Overview 面板、loader、`AppState.overview` 类型和新查询即可回到卡片墙。
