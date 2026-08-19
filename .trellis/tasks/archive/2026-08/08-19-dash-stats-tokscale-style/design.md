# Design: Dash Stats 对齐 tokscale 年历

## Architecture

本任务停在 TUI Stats 展示层、loader 字段和现有只读查询的组合。不改 `HeatmapPoint`、`/api/heatmap`、parser、store 主键。

```
usage_bucket_30m
    ├ Dashboard::heatmap(&base_filter, 365)          # 年历 / streak / active days
    ├ Dashboard::overview(&base_filter)              # tokens / events / cost
    ├ Dashboard::model_breakdown(&base_filter)       # Favorite model
    └ Dashboard::context_pressure(&window_filter)    # peak / avg
选中日
    └ Dashboard::period_model_breakdown(&day_filter) # 已有 Daily Enter 查询
         → StatsPanelPayload + PeriodDetailState
              → 年历卡 + 两列 Stats + Day Breakdown
```

## Boundaries

| 层 | 做 | 不做 |
| --- | --- | --- |
| query | 无新 SQL | 改 `HeatmapPoint`、`/api/heatmap`、加 session 计数 |
| TUI payload | `StatsPanelPayload` 去掉 `sources`/`health`，加上 `models` | 改其他面板 payload |
| loader | heatmap + overview + model_breakdown + context | 再拉 source_breakdown / health |
| Stats 面板 | 年历 + 两列 Stats + 选中后 Breakdown | Source Mix / Health Signals / 外套框 |
| 交互 | 复用 `PeriodDetailKind::Daily` | 新明细状态机；改 `h`/`l` |
| web | 无 | 改日历热力图 |

## Data flow

1. Stats 冷加载一次跑四条只读查询：`overview(base)`、`heatmap(base, 365)`、`model_breakdown(base)`、`context_pressure(window)`。
2. Stats 仍在 `panel_uses_time_window` 里，因为 context 跟窗口。切窗口会清 `stats` 并重载；年历查询仍用 `base_filter`，所以格子不变。
3. `update_scroll_total(Health)`：未选中为 0；选中后为 Breakdown 行数（见下）。
4. Enter / 点击某一天：`open_period_detail(Daily { date })`，loader 走现有 `(Some(Daily), _)` 分支。Stats 已打开明细时再点另一天：只改 `kind`、清空 `payload`、selected/offset 归零，保留当初备份的 `list_scroll`。
5. Esc / 切 tab / 切 TimeWindow：现有 `close_period_detail`。Stats 上 Esc 必须在「退出 dash」之前判定 `is_period_detail_active()`（现有顺序已满足）。
6. 鼠标：`TuiEvent::Mouse` 在 nav 命中失败后，若当前是 Stats，用与渲染相同的几何函数 `day_at(content_area, heatmap, column, row)` 得到日期再打开明细。不要给 `Action` 加 `String`（`Action` 是 `Copy`）。

## Contracts

### StatsPanelPayload

```text
StatsPanelPayload {
    overview: OverviewPayload,           # lifetime
    heatmap: Vec<HeatmapPoint>,          # 365 日补零
    models: Vec<ModelBreakdown>,         # lifetime，按 cost 取 favorite
    context_pressure: ContextPressurePayload,  # windowed
}
```

不再包含 `sources`、`health`。

### 年历几何

- 周日为一周第一天，与现有 `weekday_index` 一致。
- 把 `heatmap[0]` 之前的 weekday 补成不可点的空位。
- 每格宽 `CELL_WIDTH = 2`。可见周数 = `(inner.width - label_width) / 2`。裁左侧。
- `label_width`：宽 4（`Mon `），窄 2。
- 内容行：月份 1 + 空行 1 + 7 行格子。加上 block 边框后图卡 `Length` 约 11。
- 月份：每个可见周取该周第一天（有日期的）的月，月变化时写 `Jan`…`Dec`。
- 强度：沿用 `contribution_thresholds` / `contribution_bucket` + `theme::heat`。
- 选中日：该格用 `selection_fill_style`，符号可用 `▓▓`。
- `day_at` 与 `render_graph` 共用同一套起始 x/y 和裁剪，避免点偏。

### Stats 卡

```text
左: Favorite model | Events | Current streak | Active days N/M
右: Total tokens   | Total cost | Longest streak
然后: Context peak / avg
然后: Less ██ ██ ██ ██ More
```

- Favorite：`models.iter().max_by(|a,b| a.cost_with_cache_usd.total_cmp(&b.cost_with_cache_usd))`。名称走 `vendor_fg`。
- Events：`stat_compact(overview.total_events)`。
- tokens：`stat_compact(overview.total.total_tokens)`。
- cost：`cost_compact(overview.total_cost_usd)`。
- streak：现有 `current_streak` / `longest_streak`（按 `event_count > 0`）。
- 紧凑高度：选中且三区同屏时 Stats 卡约 8 行（可藏 Context 下一行，图例保留）。
- 未选中：Stats 卡约 10–12 行，放得下 Context。

### Day Breakdown

- 标题：`Day Breakdown (ESC to close)`。
- 把 `PeriodDetailRow` 按 `source` 分组。source 按组内 cost 降序；组内 model 按 `total_tokens` 降序。
- 展平为行向量，只格式化 `visible_range`。
- 空结果：`No data for this day`。
- 首行日期用 heatmap 的 `YYYY-MM-DD` 格式化为 `%a, %b %d, %Y`。

### 高度分配

```text
min_graph = 11
stats_full = 12
stats_compact = 8
min_breakdown = 6

未选中:     graph Length(min_graph) + stats Length(stats_full)   # 剩余留白
选中且够高: graph Length(min_graph) + stats Length(stats_compact) + breakdown Min
选中且不够: graph Min(min_graph) + breakdown Length(12)
```

「够高」：`area.height >= min_graph + stats_compact + min_breakdown`。

## Compatibility

- `/api/heatmap` 与 `HeatmapPoint` 字段不变。
- Daily Enter 的 `period_model_breakdown` 与 `PeriodDetailKind::Daily` 语义不变。
- `data_loader` 里 Stats 并行对照测试改为新 payload 字段。
- `dashboard-performance-contracts.md`：Stats 窗口化范围从「source mix / context」改为「context」；heatmap 仍固定 365 日。
- `tui-presentation-contracts.md`：写明 Stats 年历 + 两列卡 + Day Breakdown。
- `tui-runtime-contracts.md`：Stats 不再滚 Source Mix；选中后 `ScrollState` 滚 Breakdown；Esc 关选中。

## Trade-offs

- 用 Events 代替 Sessions，避免新的 session 口径查询。
- Favorite model / 合计走 lifetime，Context 走窗口：与改前 Stats 的 overview vs context 分工一致。
- 格子移动只靠点击和 Enter 回今天。`h`/`l` 继续切窗口，避免和全局快捷键打架。
- 去掉 sources/health 查询，Stats 冷加载少两条 SQLite。diagnostics 仍在 Usage / 同步面。

## Rollback

无 migration。还原 `src/tui/panels/stats.rs`、`StatsPanelPayload`、loader、draw/mouse/help 和三份 contract 即可回到旧 Stats。
