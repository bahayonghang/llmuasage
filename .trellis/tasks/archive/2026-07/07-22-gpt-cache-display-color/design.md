# 技术设计

## 边界

只改 `src/tui/report_table.rs`（CLI 人类表格投影层）。解析器、query DTO、
CLI JSON、dashboard/TUI/export 序列化一律不动。遵循
`.trellis/spec/llmusage/backend/report-cli-contracts.md` 与
`tui-presentation-contracts.md`。

## 方案

### 1. cache 零值占位符

新增纯函数 `format_cache_cell(tokens: i64) -> String`：
`0 -> "-"`，非零走 `format_token_compact`。应用于：

- `unified_row` / `unified_total_row`
- `focused_row` / `focused_total_row`
- `append_unified_breakdowns` / `append_focused_breakdowns`

仅 Cache Create 与 Cache Read 两列。Total Tokens 仍按
`input + output + cache_creation + cache_read` 饱和求和，与占位符无关。
daily-summary / daily-source / session / blocks 表列格式不动（非目标）。

### 2. 表格样式泛化

现有 `DailyTableStyle { source, color_mode }` 仅服务 per-source 日报表。
泛化为：

```rust
struct TableStyle {
    color_mode: ColorMode,
    accent: Option<Color>, // None = 终端默认色，仅加粗
    notes_dim: bool,       // 保留 daily-source 表 Notes 列 dim 行为
}
```

- daily-source 表：`accent = Some(source_color(source))`，`notes_dim = true`
  —— 输出与现状逐字节一致。
- unified 表：`accent = None`（多源表不偏向单一源色），表头/Total 行加粗；
  Agent 标签沿用现有 `style_unified_source_labels` 按源着色。
- focused 表：`accent = Some(source_color(source))`，与标题同色。
- `RowStyle.color: Color` 改为 `Option<Color>`。

### 3. 占位符 dim

`push_styled_padded` 增加判断:颜色启用且单元格内容为 `"-"` 且列为右对齐
数值列时,以 dim 样式输出（Total 行为 bold + dim 叠加）。
ColorMode::Never / 非 TTY 下输出纯文本 `-`，无 ANSI。

## 兼容性

- 现有测试中 unified 非零 cache 断言（7.85M/134.61M）不受影响；
  `unified_table_renders_all_agent_rows_and_detected_title` 的 cache 为 0，
  改后渲染 `-`，该测试未断言 `0`，不破坏。
- `NO_COLOR` / `LLMUSAGE_NO_COLOR` / 管道输出（非 TTY Auto）自动降级纯文本。
- JSON 快照与 `tests/report_commands.rs` 的数值断言不受影响。

## 回滚

单文件展示层改动，`git revert` 即可，无数据/schema 影响。
