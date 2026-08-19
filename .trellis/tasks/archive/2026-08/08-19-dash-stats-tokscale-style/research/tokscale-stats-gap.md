# tokscale Stats vs llmusage dash Stats

对照日期：2026-08-19

对照来源：用户两张截图、`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/stats.rs`、
`data/mod.rs` 的 `build_contribution_graph_for_today`、`src/tui/panels/stats.rs`、
`src/query/heatmap.rs`、`src/tui/data_loader.rs`、
`.trellis/spec/llmusage/backend/tui-presentation-contracts.md`、
`.trellis/spec/llmusage/backend/dashboard-performance-contracts.md`。

同类已归档任务：`08-18-dash-models-tokscale-style`、
`08-19-dash-overview-tokscale-style`、`08-19-dash-usage-tokscale-style`、
`08-19-dash-period-tokscale-style`。那些任务只改对应 dash 页，不改 web。

---

## tokscale Stats（截图 #2 + 源码）

源码：`ref/repo/tokscale/crates/tokscale-cli/src/tui/ui/stats.rs`。

| 区域 | 行为 |
| --- | --- |
| 默认布局 | 上图 `Min(12)` + 下 Stats 卡 `Length(12)` |
| 选中格子 | 高度够时：图 + 8 行紧凑 Stats + Day Breakdown；不够则图 + Breakdown，藏 Stats |
| 图标题 | `Contribution Graph (52 weeks)` |
| 格子 | 宽 2 列 `██`；选中 `▓▓` 反色；无数据 `· ` |
| 轴 | 顶行月份缩写；左侧隔行 weekday（Sun 起，显示 Mon/Wed/Fri） |
| 窗口 | 从「本周日往前 364 + 到周日的天数」铺满周列，按本地日历补零 |
| 强度 | `day.cost / max(cost)`，再切成 5 档主题色 |
| Stats 卡 | 两列键值：Favorite model / Sessions / Current streak / Active days N/M；右侧 Total tokens / Total cost / Longest streak |
| 图例 | Stats 卡内 `Less · ██ ██ ██ ██ More` |
| 标语 | 宽屏斜体 `Your total spending is $x.xx on AI coding assistants!` |
| 明细 | 选中日后按 source → model 列出 In/Out/CR/CW 与成本 |

截图里图卡下方仍有大块空白：7 行日历只占图卡顶部，剩余高度被 `Min` 吃掉。这是 tokscale 自己的排版浪费，不应当原样复制。

---

## 当前 llmusage Stats（截图 #1 + 源码）

源码：`src/tui/panels/stats.rs`。面板枚举仍是 `Panel::Health`，显示名 `Stats`。

| 区域 | 行为 |
| --- | --- |
| 外框 | 整页一块 `panel_block("Stats")`，内再套三张 `trend_card_block` |
| 顶栏 | 固定 5 行，4 行指标挤在一起：tokens / events / cost、active days / streak / best day、sources / cache / failures、context peak / avg / unknown |
| 贡献图 | 高度 ≥24 时固定 10 行，否则 6 行；格子宽 1（`■`）；无月份、无 weekday |
| 日期说明 | `compact_date` 只取 `MM-DD`。365 日窗若从去年 08-20 到今年 08-19，会显示 `08-20 .. 08-19` |
| Source Mix | `Constraint::Min(5)` 吃掉剩余高度；列 Source / Tokens / Events / Last Event / Profile（`#---` 条） |
| Health Signals | 固定 4 行：cursors、failures，再加一句说明。failures 与顶栏重复 |
| 热力分档 | 正 token 日的 P25/P50/P75/P99，不是 tokscale 的 max-cost 归一 |
| 数据 | `heatmap(base_filter, 365)` 固定 365 日；`overview` 用 lifetime；`source_breakdown` 与 `context_pressure` 跟 TimeWindow |
| 交互 | `ScrollState` 只滚 Source Mix。鼠标左键只切 tab，不能点格子 |

`HeatmapPoint` 只有 `date` / `event_count` / `total_tokens`，没有 cost。
`/api/heatmap` 序列化同一结构，TUI 不能改这个字段集。

---

## 截图问题（按严重度）

1. **主次反了。** Source Mix 只有 9 行，却占整页最大区域，表体全是空。贡献图被压在约 10 行里。tokscale 把年历当作本页主体。
2. **年历不可读。** 无月份、无 weekday、格子宽 1。宽终端能画很多列，但看不出哪一段是哪个月。
3. **日期说明像反了。** `08-20 .. 08-19` 是去了年份的 365 日窗，不是数据倒序。
4. **顶栏密度过高。** 4 行无标签列对齐的指标，cache / failures / context 与 Health 卡抢注意力。
5. **Health Signals 信息量低。** 4 行里有重复的 failures 和一句占位说明。
6. **Profile 条简陋。** `#` / `-` 与其他页的块字符不一致。
7. **套娃边框。** 外 `Stats` + 内三卡，有效内容行被边框吃掉。
8. **不要照抄 tokscale 的图卡空白。** 年历本身只有「月份 + 7 行格子」。图卡用 `Min` 拉高后，截图 #2 下半截是空的。优化应把图卡高度钉在年历所需行数，而不是把空白留在图卡内部。

---

## 合同约束

- 365 日 heatmap 不跟 TimeWindow。`dashboard-performance-contracts.md`：切 `All → 30d` 时 Overview 合计和 365 日 heatmap 保持稳定；Stats 的 source mix / context pressure 才重载。
- Stats 的 tokens / 计数走 `stat_compact`；成本展示可复用 `cost_compact`。
- 面板禁止 `Color::*`。热力色走 `theme::heat(0..=4)`。
- 交互文案保持英文。
- 不改 `/api/heatmap`、`/api/overview`、`HeatmapPoint` 已有字段。
- 不改 web 日历热力图。

---

## 可复用

- `theme::heat` 五档色。
- `stat_compact` / `cost_compact`。
- `weekday_index` / `contribution_thresholds` / `contribution_bucket` / streak 函数与单测。
- Overview 已加载的 `model_breakdown`（Favorite model、Sessions 可同源，Stats 今日未加载）。
- `ScrollState`（若保留名单或明细）。

---

## 缺口

1. 没有 52 周 `weeks[week][weekday]` 结构；现在是 365 个点从左上按周日对齐后裁右侧可见列。
2. 没有月份轴、weekday 轴、双列格子。
3. 没有两列键值 Stats 卡。
4. 图卡高度按终端高度给固定 10，而不是按年历内容。
5. 没有格子选中。llmusage 鼠标没有单元格 hit-test。
6. 没有按日 source×model 明细查询。
7. `tests/tui_panels_prop.rs` 锁死了 `Source Mix`、`Health Signals`、`current streak` 文案。

---

## 构图选项

| 选项 | 页面结构 | 状态 |
| --- | --- | --- |
| A | 年历（按内容定高）+ 两列 Stats | 未选 |
| B | A + 按行数定高的 Source Mix | 未选 |
| C | A + 选中日后 Day Breakdown | **已选 2026-08-19** |

## Stats 指标（已选 A，2026-08-19）

两列卡：Favorite model（lifetime `model_breakdown` 按 cost）、Events（`overview.total_events`，不做 Sessions）、Current / Longest streak、Active days `N/M`、Total tokens、Total cost；另留 Context peak / avg（仍跟 TimeWindow）。

---

## 选 C 后的交互与数据（已核对）

tokscale 选格几乎只靠鼠标：`ClickAction::GraphCell`。Enter 在已选中时只写 status `Press ESC to deselect`。j/k 在选中后滚 Day Breakdown，不移动格子。Esc 清掉 `selected_graph_cell`。

llmusage 现状：

- 鼠标左键只切 tab（`action_from_mouse`），没有单元格 hit-test。
- `h` / `l` / 左右箭头已经是 TimeWindow，不能无条件改成移格子。
- Daily Enter 已经有 `period_model_breakdown` → `PeriodDetailRow`（model × source、Msgs、四通道、Cost）。
- Esc 在无 period detail 时退出 dash；有 detail 时关闭明细。
- `ModelBreakdown` 没有 `session_count`。tokscale 的 Sessions 是各模型 session 之和。llmusage 的 `session_count` 在 Behavior/Tools，不在模型合计。
- `context_pressure` 只在 Stats 面板渲染。Overview 已去掉 KPI 卡，删掉这行后 TUI 不再显示上下文占用。

Day Breakdown 不必新 SQL：按选中日把 `QueryFilter.since/until` 收成当天，复用 `period_model_breakdown`。渲染可按 source 分组，做成 tokscale 的 source → model 树。
