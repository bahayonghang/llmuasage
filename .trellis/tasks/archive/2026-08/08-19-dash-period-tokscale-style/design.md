# Design: Dash Daily / Hourly / Monthly 对齐 tokscale 周期表

## Architecture

本任务停在 TUI 周期表、tab 枚举，以及只读查询扩展。解析器、store 主键、定价目录、web JSON 都不改。

```
usage_bucket_30m + usage_turn
    → Dashboard::trends_daily / trends_hourly / trends_monthly
    → TUI Daily / Hourly / Monthly
         ├ format::{stat_compact, cost_compact, cache_multiplier, cost_per_million}
         ├ theme metric slots
         └ Enter → period_model_breakdown | trends_daily(month bounds)
```

`Dashboard::trends("hourly")` 和 `TrendPoint` 只留给 `/api/trends`。TUI Hourly 不再调用它。

## Boundaries

| 层 | 做 | 不做 |
| --- | --- | --- |
| timezone | 新增 `local_hour_expr` / `llmusage_local_hour`，格式 `YYYY-MM-DD HH:00` | 改 date/month/week 标量语义 |
| query | `trends_hourly`、`trends_monthly`；Daily/Hourly/Monthly 合并 Turn；`period_model_breakdown` | 改 `trends()`、`/api/trends*`、`cost_breakdown` |
| TUI nav | `Panel::Cost` 换成 `Panel::Monthly`；仍 9 项 | 重命名 `Sources`/`Projects` |
| Daily/Hourly/Monthly 面板 | tokscale 列、Hourly 日期分隔、Enter 明细 | Hourly Profile、Minutely |
| Cost 面板 | 删除 `panels/cost.rs` 与 TUI 接线 | 删除 `CostLine`、`/api/costs`、`longtail` 的测试夹具若无引用则随 Cost 删除 |
| web / CLI | 不改 | — |

## Tab 与枚举

`Panel::COUNT` 仍为 9。

| 下标 | 变体 | 显示名 | 数字键 |
| --- | --- | --- | --- |
| 0 | Overview | Overview | 1 |
| 1 | Trends | Usage | 2 |
| 2 | Models | Models | 3 |
| 3 | Sources | Daily | 4 |
| 4 | Projects | Hourly | 5 |
| 5 | Monthly（原 Cost） | Monthly | 6 |
| 6 | Health | Stats | 7 |
| 7 | Behavior | Agents | 8 |
| 8 | Blocks | Blocks | 9 |

`Panel::Sources` / `Panel::Projects` 保持内部名，只改 `label`/`short_label`（已是 Daily/Hourly）。`short_label` Monthly = `Mon`。

删除 `AppState.costs`、`cost_collapse`、`PanelPayload::Costs`。Cost 面板是 `longtail` 的唯一调用方，一并删除 `panels/longtail.rs`。

## Data flow

### 列表查询

三个列表都从 `usage_bucket_30m` 聚合，再按同一时区键左连 Turn 计数。

| 方法 | 分组键 | 行类型 |
| --- | --- | --- |
| `trends_daily`（扩展） | `local_date_expr(hour_start)` | `DailyTrendPoint` + `turn_count`（`serde(default, skip_serializing)`） |
| `trends_hourly`（新） | `local_hour_expr(hour_start)` | `HourlyTrendPoint` |
| `trends_monthly`（新） | `local_month_expr(hour_start)` | `MonthlyTrendPoint` |

桶查询选出：input / cache_read / cache_creation / output / total / event_count / cost，Hourly 另加 `GROUP_CONCAT(DISTINCT source)`，Rust 侧 split/trim/sort/dedup，与 Models 相同。

Turn 查询：

```sql
SELECT {period_expr} AS key, COUNT(*)
FROM usage_turn t
{turn_filter}
GROUP BY key
```

`period_expr` 用 `started_at`。在 Rust 里按 key 填 `turn_count`，缺省 0。禁止把 `usage_turn` join 进桶查询。

`HourlyTrendPoint`：

```text
hour_start: String   # 本地 "YYYY-MM-DD HH:00"
input/output/cache_read/cache_creation/total: i64
event_count: i64
turn_count: i64
cost_with_cache_usd: f64
sources: Vec<String>
```

`MonthlyTrendPoint`：`month: String`（`YYYY-MM`）+ 与 Daily 相同的度量字段。两类型不进 web JSON。

### 时区小时

`ResolvedZone::local_hour_expr`：

- Fixed：`strftime('%Y-%m-%d %H:00', col, '+N seconds')`
- IANA：`llmusage_local_hour(col, zone)`，在 `register_functions` 注册，格式与 Fixed 相同

DST 与 `local_date_expr` 同一套 `date_at`。测试覆盖纽约春播/秋回附近的 `hour_start`。

### Enter 明细

不把全年 model×source 塞进列表 payload。

| 入口 | 查询 |
| --- | --- |
| Daily Enter | `period_model_breakdown(filter)`，`since = until = 选中日`，`GROUP BY model, source` |
| Monthly Enter | `trends_daily(filter)`，`since`/`until` 为该月首末日 |

`PeriodDetailRow`：model, source, event_count, 四通道, total, cost。TUI 算 Cache× / Cost/1M。Daily 明细宽表不强制 Provider/#（tokscale 有；本任务用 Model/Source 即可，避免再引厂商色进周期页）。

`AppState`：

```text
period_detail: Option<PeriodDetailState>
```

`PeriodDetailState` 分 `Daily { date, rows }` 与 `Monthly { month, days }`。进入时保留列表 payload 和列表 `ScrollState`，明细用同一 `scroll[panel]` 窗口（进入时 `selected/offset = 0`，Esc 恢复进入前的列表滚动）。

切 tab、改 TimeWindow、改 source、`r` 刷新列表时清掉 `period_detail`。

加载：现有 `PanelDataLoader` 增加 `PanelPayload::DailyDetail` / `MonthlyDetail`。请求期间主列表仍显示，明细区可先 `Loading...`。

### Hourly 日期分隔

`ScrollState.total` 仍是小时行数。渲染时按可见数据行预算高度：窗口顶和跨日处插入 `%m/%d` 分隔行。只剩 1 行时丢掉分隔、仍画数据行（tokscale 回归）。分隔行不可选。当前本地小时用 `theme` 的 warning/peak 槽高亮时间列，不写 `Color::Yellow`。

`update_scroll_total(Hourly)` = `hourly.len()`，不是含分隔的视觉行数。

## Presentation

共享 `src/tui/panels/period.rs`（或 `period_table.rs`）纯函数：宽/窄表头、Turn 列是否出现、metric cells、widths。Daily / Monthly / Monthly-detail 调用。Hourly 因 Source 列和分隔行留在 `hourly.rs`，复用 metric cells。

宽度档与 Models/tokscale 相同：inner width `<60` / `<80`。宽表总宽不够时 Daily 日期用 `MM-DD`、列宽 7。

Cost 数字用 `cost_compact`（与 Models 一致）。Msgs/Turn 用 `stat_compact`。

选中行：周期表无厂商色，用 `selection_style()`。明细里模型名可用 `vendor_style`（已有纯函数），不是本任务必做；为少改着色合同，明细模型名不着色。

## Input

`handle_key_event`：

- `Enter` → `Action::OpenDetail`（Hourly/其他页由 `mod.rs` 忽略）
- `Esc` → `Action::Esc`（不再直接 Quit）
- `q` → `Action::Quit`

`mod.rs`：`Esc` 时若 `period_detail` 有值则关闭明细并恢复列表滚动；否则 Quit。

Help：`window: Models/Daily/Hourly/Monthly/Stats/Behavior`；补 `enter: day/month detail`。

## Compatibility

| 表面 | 合同 |
| --- | --- |
| `/api/trends` | 仍 `trends()` + `PublicTrendPoint` |
| `/api/trends_daily` | `DailyTrendPoint` 公开字段不变；`turn_count` skip |
| `/api/costs` | 不变 |
| CLI monthly JSON | 不变 |
| TUI Cost tab | 删除 |

## Risks

- All 窗口 Hourly 行数大：只格式化可见行；查询仍一次聚合。若 All 过慢，不在本任务做分页 SQL。
- Turn 查询失败不应让列表失败：Turn 计 0，Turn 列隐藏。
- Hourly 分隔行与 `visible_range` 不同步会让选中行消失：用 tokscale 的“丢分隔、保数据行”规则，并加单行视口测试。
