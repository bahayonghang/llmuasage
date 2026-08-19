# Implement: Dash Overview 对齐 tokscale 图表首页

## Checklist

1. **Query**
   - 在 `src/query/mod.rs` 增加 `DailyModelPoint` 与 `Dashboard::trends_daily_by_model`。
   - SQL：`local_date` + `model` + `SUM(total_tokens)`，过滤走 `bucket_filter`。
   - 单测：时区切日、source 过滤、空库。

2. **TUI payload and loader**
   - 增加 `OverviewPanelPayload { totals, daily_models, models }`。
   - `AppState.overview` 改为该类型。
   - loader 一次请求跑 `overview` + `trends_daily_by_model` + `model_breakdown`。
   - Overview 纳入 `panel_uses_time_window` 和 `invalidate_windowed_panel_data`。
   - footer 改读 `totals`。
   - `update_scroll_total` 用 `models.len()`。

3. **Theme**
   - 增加 `vendor_fg(vendor, rank) -> Color`：const 坡度 → `adapt_color`。`NoColor` 返回 `Color::Reset` 或与现有 NoColor 契约一致的无色。
   - `vendor_style` 继续用同一坡度。单测 NoColor / ANSI16。

4. **Stacked bar**
   - 新增 `src/tui/stacked_bar.rs`（或 `panels` 旁的纯渲染模块）。
   - 输入：日期、每段 `(tokens, Color)`、最大值。
   - 不写 `Color::*` 字面量。块字符逻辑对齐 tokscale 的 8 档。
   - 单测：空数据、单日单模型、多模型堆叠、窄宽标签。

5. **Overview panel**
   - 重写 `src/tui/panels/overview.rs`：柱图 35% + 图例 + 双行名单。
   - `render` 接收 payload、`ScrollState`、`SortState`。
   - 默认 Cost 降序。宽/窄第二行格式按 R4。
   - shade 表用完整 models 列表建一次。

6. **Sort / scroll wiring**
   - `sort_keys(Overview) = [Tokens, Cost]`。
   - `AppState::new` 给 Overview 设 `cost_desc()`。
   - `draw.rs` 传入 scroll / sort。
   - 确认 `j/k/Pg` 滚动名单，不滚柱图。

7. **Contracts and tests**
   - 更新 `tui-presentation-contracts.md`：Overview 构图、`vendor_fg`、`stat_compact` 仍用于轴标签。
   - 更新 `tui-runtime-contracts.md`：Overview 可滚动、可排序、跟随 TimeWindow。
   - 改 `tests/tui_panels_prop.rs`：删 Token Mix / 24h Pulse 断言；加新标题、名单、NoColor、可见行窗口。
   - 更新所有 `AppState.overview` / `render_overview_text` 构造点。

8. **Gate**
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - Overview / query / tui 相关测试，再全量。

## Validation

```text
cargo test --lib query -- --test-threads=1 --exact trends_daily_by_model
cargo test trends_daily_by_model -- --test-threads=1
cargo test --lib tui::theme -- --test-threads=1
cargo test --lib tui::app -- --test-threads=1
cargo test --test tui_panels_prop -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

实现完成后用 `llmusage dash` 打开 Overview，核对照片级观感：堆叠柱、图例、`Models by Cost`、无 KPI 卡。

## Risky files

| 文件                         | 风险                                                     |
| ---------------------------- | -------------------------------------------------------- |
| `src/query/mod.rs`           | 新类型的字面量要补全测试夹具                             |
| `src/tui/app.rs` / `mod.rs`  | 漏改 `overview` 类型或 TimeWindow 集合会导致切窗口不刷新 |
| `src/tui/footer.rs`          | 仍读旧 `OverviewPayload` 会编不过                        |
| `src/tui/panels/overview.rs` | 直接写 `Color::*` 会违契约                               |
| `tests/tui_panels_prop.rs`   | 旧 KPI 断言会红                                          |

## Rollback

无 migration。还原上述文件即回到卡片墙 Overview。

## Before start

- 用户已批准本规划摘要。
- 先读 `tui-presentation-contracts.md`、`tui-runtime-contracts.md`、`research/tokscale-overview-gap.md`。
- 不要开 web 首页、Models 宽表、hourly 粒度的连带改动。
