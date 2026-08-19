# Implement: Dash Daily / Hourly / Monthly 对齐 tokscale 周期表

## Checklist

1. **时区小时键**
   - 在 `src/query/timezone.rs` 加 `local_hour_expr` 与 `llmusage_local_hour`。
   - Fixed 与 IANA 都输出 `YYYY-MM-DD HH:00`。
   - 单测：固定偏移；纽约 DST 春播/秋回附近的 `hour_start`。

2. **查询**
   - 扩展 `trends_daily`：附加 `turn_count`（`serde(default, skip_serializing)`）。
   - 新增 `HourlyTrendPoint`、`MonthlyTrendPoint`、`PeriodDetailRow`。
   - 新增 `trends_hourly`、`trends_monthly`、`period_model_breakdown`。
   - Turn 单独 `GROUP BY` `usage_turn.started_at`，按 key 合并；查询失败当 0。
   - 单测：30 分钟两桶并进同一本地小时；跨时区日期/小时；Turn 有/无；`trends_daily` JSON 不含 `turn_count`。

3. **Panel 枚举**
   - `Panel::Cost` → `Panel::Monthly`，`label`/`short_label` = Monthly / Mon。
   - 删除 `AppState.costs`、`cost_collapse`。
   - `sort_keys`：Daily / Hourly / Monthly / Blocks 均为 Date、Tokens、Cost。
   - 默认仍 Date 降序（`SortState::default()` 即可，Daily 已 `rev()`；改为显式 Date desc，三页一致）。

4. **加载与明细状态**
   - `PanelPayload`：Hourly 改为 `Vec<HourlyTrendPoint>`；Costs 换成 Monthly；加 DailyDetail / MonthlyDetail。
   - `data_loader`：Daily/Hourly/Monthly 走新查询；明细请求带收紧后的 `QueryFilter`。
   - `AppState.period_detail` + 进入前滚动快照。
   - 切 tab / 改窗口 / 改 source / 刷新列表时清明细。

5. **输入**
   - `Enter` → `OpenDetail`；`Esc` → `Esc`；`q` → `Quit`。
   - 仅 Daily/Monthly 列表响应 Enter。明细中 Esc 返回。
   - `input.rs` / `mod.rs` 测试覆盖。

6. **呈现**
   - 抽 `src/tui/panels/period.rs`：宽窄表头、Turn 显隐、metric cells。
   - 重写 `daily.rs`：新列、去 footer、Enter 明细。
   - 重写 `hourly.rs`：整点表、Source、日期分隔、当前小时高亮。
   - 新增 `monthly.rs`：同 Daily 列，Month 键；Enter 日表。
   - `draw.rs` / `footer.rs` / `help_dialog.rs` 去掉 Cost、写上 Monthly 与 Enter。
   - 删除 `panels/cost.rs`、`panels/longtail.rs` 及其 `mod` 导出。

7. **合同与测试清理**
   - `tui-presentation-contracts.md`：九面板列表把 Cost 换成 Monthly；周期表用 Cache× / Cost/1M。
   - `tui-runtime-contracts.md`：Hourly/Monthly 加入 SortState；Cost 折叠段落删掉。
   - `tests/tui_panels_prop.rs`：Cost 策略改 Monthly；Hourly 不再断言 Share/Profile。
   - theme 九面板壳测试随 `Panel::all()` 自动换标签。

8. **验证**
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - 先跑：`cargo test --all-features -- --test-threads=1 timezone trends_daily trends_hourly trends_monthly tui_panels`
   - 再跑：`cargo test --all-features -- --test-threads=1`

## Risky files

| 文件 | 风险 |
| --- | --- |
| `src/tui/app.rs` `Panel` | 数字键、滚动数组、属性测试 |
| `src/query/timezone.rs` | DST 小时桶错位 |
| `src/tui/input.rs` Esc | 误把全局退出改坏 |
| `src/tui/panels/hourly.rs` | 分隔行与选中行不同步 |
| `tests/tui_panels_prop.rs` | 仍引用 `CostLine` / `TrendPoint` 渲染 |

## Rollback

- 查询扩展向后兼容：去掉 TUI 字段即可回到旧 Daily。
- `trends()` / `/api/costs` 未改，web 不回滚。
- Cost 面板删除不可热回退；回退靠 git revert 本任务提交。

## Follow-up before `task.py start`

- [x] PRD 无未决问题
- [x] design.md / implement.md 已写
- [ ] 用户确认本规划摘要后才能 `task.py start`
