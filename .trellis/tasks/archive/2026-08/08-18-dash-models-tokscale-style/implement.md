# Implement: Dash Models 对齐 tokscale 彩色表

## Checklist

1. **Query sources**
   - 给 `ModelBreakdown` 加 `sources: Vec<String>`。
   - `model_breakdown` SQL 增加 `GROUP_CONCAT(DISTINCT source)`，Rust 侧 split / trim / sort / 去重。
   - `PublicModelBreakdown` 与 `From` 不拷贝 `sources`。
   - 更新 `arb_model_breakdown` 及所有 `ModelBreakdown { ... }` 结构字面量。
   - 给跨 source 同一 model 加一条 query 测试。

2. **Format helpers**
   - 在 `src/tui/format.rs` 增加 `cost_compact`、`cache_multiplier`、`cost_per_million` 及单测。
   - 不改 `cost` / `stat_compact` 的现有断言。

3. **Vendor identity**
   - 新增纯模块（建议 `src/tui/model_vendor.rs`）：`vendor_from_model`、`vendor_display_name`、`build_shade_map`。
   - 单测：fable/opus 色阶、`unfabled` 不进 Anthropic、网关式名字仍跟厂商走、gpt-4o 版本解析。

4. **Theme accessors**
   - `theme.rs` 增加 `metric_cache_hit`、`metric_cost_per_million`，更新四套主题、`adapted`、槽覆盖测试。
   - `default_dark` 既有槽保持历史值。
   - 增加 `vendor_style(vendor, rank)`：const 坡度 → `adapt_color` → `bold_fg_style`。面板只调 accessor。

5. **Models panel**
   - 按 `area.width` 分三档列集。宽表 13 列，含 `#` / Provider / Source / 通道 / Cache× / Cost/1M。
   - 单元格分色；选中行保留单元格 fg。
   - 不再 truncate 成长尾。`collapse_plan` 可删或恒返回 `None`，并改掉 `apply_panel_result` / `update_scroll_total`。
   - `render` 测试入口使用 Cost 降序，与运行时默认一致。

6. **Default sort**
   - `AppState::new`：`sort[Models] = { Cost, descending }`。
   - 表头 `Cost ▼`。确认 `sort_state_is_remembered_per_panel` 仍过。

7. **Contracts and tests**
   - 更新 `tui-runtime-contracts.md`：仅 Cost 折叠；Models 始终原始行数。
   - 更新 `tui-presentation-contracts.md`：新 helper 与新槽的签名/测试要求。
   - 改 `tests/tui_panels_prop.rs`：成本断言改 `cost_compact`；补宽/窄表头、无 `+N more`、NoColor。
   - 必要时补 TestBackend 宽/窄/NoColor 用例。

8. **Gate**
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test -- --test-threads=1` 中 TUI/query 相关切片，再全量。

## Validation

```text
cargo test --lib tui::format -- --test-threads=1
cargo test --lib tui::model_vendor -- --test-threads=1
cargo test --lib tui::theme -- --test-threads=1
cargo test --lib tui::app -- --test-threads=1
cargo test --test tui_panels_prop -- --test-threads=1
cargo test model_breakdown -- --test-threads=1
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

实现完成后用 `llmusage dash` 打开 Models，核对照片级观感：Cost ▼、厂商色、无 `+N more`、窄终端降列。

## Risky files

| 文件 | 风险 |
| --- | --- |
| `src/query/mod.rs` | `ModelBreakdown` 字面量全库要补 `sources` |
| `src/tui/theme.rs` | 漏改 `adapted` 会编不过或 ANSI16 测试漏槽 |
| `src/tui/panels/models.rs` | 宽表列太多时窄于 80 仍走宽约束会挤掉数字 |
| `src/tui/mod.rs` | 漏掉 `model_collapse` 行数会让滚动总量多 1 |
| `tests/tui_panels_prop.rs` | 仍断言 `{:.4}` 成本会红 |

## Rollback

无 migration。还原上述文件即可回到 4 列单色 + 长尾折叠。

## Before start

- 用户已批准本规划摘要。
- 先读 `tui-presentation-contracts.md`、`tui-runtime-contracts.md`、`research/tokscale-models-gap.md`。
- 不要开 parser / 定价 / web Models 的连带改动。
