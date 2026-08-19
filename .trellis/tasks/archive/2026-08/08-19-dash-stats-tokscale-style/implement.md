# Implement: Dash Stats 对齐 tokscale 年历

## Checklist

1. **Payload / loader**
   - `StatsPanelPayload` 改为 `{ overview, heatmap, models, context_pressure }`。
   - `load_stats_panel_data`：`overview(base)` + `heatmap(base, 365)` + `model_breakdown(base)` + `context_pressure(window)`。去掉 source / health。
   - 更新 `data_loader` 并行对照与构造点。

2. **年历几何**
   - 在 `stats.rs` 抽出 `GraphLayout`：label 宽、起始 x/y、可见周、`day_at(x, y) -> Option<NaiveDate>`。
   - 渲染：月份、隔行 weekday、2 列 `██` / `· `、选中反色。
   - 单测：周日对齐、裁左侧、命中与绘制同一格、空 heatmap。

3. **两列 Stats 卡**
   - 按 R2/R3 画标签。窄档 `<80`。
   - Favorite / Events / streaks / `N/M` / tokens / `cost_compact` / Context / 图例。
   - 删除 Source Mix、Health Signals、外套 `panel_block("Stats")`、caption `MM-DD .. MM-DD`、`render_bar`。

4. **选中 + Day Breakdown**
   - `period_detail_kind`：`Panel::Health` 且无明细时返回 `Daily { heatmap.last().date }`。
   - 已打开时点击另一天：换 `kind`、清 `payload`、滚动归零，不覆盖 `list_scroll` 备份。
   - `draw.rs` 把 `period_detail` 传给 stats。
   - 按 source 分组展平行；宽/窄通道格式与 Overview 名单一致。
   - `update_scroll_total(Health)` 用 Breakdown 行数。
   - 高度分配按 design。

5. **鼠标**
   - `mod.rs` 在 nav 未命中且 `Panel::Health` 时调用 `day_at`，命中则 `request_period_detail`。
   - 单测：给定 area + heatmap，点某格得到对应日期。

6. **Help / contracts**
   - Help 增加 Stats：click day / Enter today / Esc close。
   - 更新 `tui-presentation-contracts.md`、`tui-runtime-contracts.md`、`dashboard-performance-contracts.md`（Stats 窗口化只剩 context）。

7. **Tests**
   - 改 `tests/tui_panels_prop.rs` 的 `sample_stats_payload` 和旧 Source Mix / Health 断言。
   - 覆盖 AC1–AC6 文案、窄标签、Enter/Esc、空日、NoColor。
   - 保留 streak / bucket 单测。

8. **Gate**
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - 相关测试后再全量。

## Validation

```text
cargo test --lib tui::panels::stats -- --test-threads=1
cargo test --lib tui::data_loader -- --test-threads=1
cargo test --lib tui::app -- --test-threads=1
cargo test --test tui_panels_prop -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

实现后用 `llmusage dash` 打开 Stats：52 周年历、两列摘要、点一天出 Breakdown、Esc 回到年历。

## Risky files

| 文件 | 风险 |
| --- | --- |
| `src/tui/panels/stats.rs` | 几何和 `day_at` 不一致会导致点错天 |
| `src/tui/app.rs` | payload 字段漏改编不过；明细二次打开冲掉 `list_scroll` |
| `src/tui/mod.rs` | Esc 顺序错会退出 dash；鼠标抢 nav 点击 |
| `src/tui/data_loader.rs` | 并行对照仍读 `sources`/`health` |
| `tests/tui_panels_prop.rs` | 旧 Source Mix / Health 断言会红 |
| 三份 contract | 仍写「Stats source mix」会与实现漂移 |

## Rollback

无 migration。还原上述文件即回到旧 Stats。

## Before start

- 用户已批准本规划摘要。
- 先读 `tui-presentation-contracts.md`、`tui-runtime-contracts.md`、`dashboard-performance-contracts.md`、`research/tokscale-stats-gap.md`。
- 不要改 `/api/heatmap`、web 日历、Daily 表、Sessions 查询。
