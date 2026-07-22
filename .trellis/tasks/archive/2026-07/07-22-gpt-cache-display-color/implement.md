# 执行清单

1. [x] `src/tui/report_table.rs`：新增 `format_cache_cell`，替换
       unified/focused 行、Total 行、model breakdown 行的两个 cache 单元格。
       → 验证：`cache_cell_renders_placeholder_for_zero_and_compact_otherwise`、
       `unified_zero_cache_cells_render_placeholder_and_nonzero_stay_compact`。
2. [x] 泛化 `DailyTableStyle` → `TableStyle`（`accent: Option<Color>` +
       `notes_dim` + `dim_placeholders`），unified/focused 渲染接入表头/Total
       加粗样式，`-` 单元格 dim。
       → 验证：`unified_table_always_color_bolds_header_total_and_dims_placeholders`、
       `focused_table_ansi_styles_follow_color_mode`；daily-source 表既有测试不变。
3. [x] 更新/新增 report_table.rs 单元测试（14 passed）。
4. [x] 全量质量门：fmt clean、clippy 无 issue、report_commands 22 passed、
       单线程全量 499 passed。并行下 3 个 flaky 失败为预先存在的全局状态竞争
       （tui/mod、query/mod），与本改动无关。
5. [x] Spec 更新：`report-cli-contracts.md` 补充 cache 占位符与
       表格颜色投影契约。
6. [ ] cargo fmt 后提交（待用户确认；见 memory：格式化 hook 会重排 use 导入）。

## 回滚点

- 步骤 1-2 均在单文件内，任一验证失败直接还原 `src/tui/report_table.rs`。
