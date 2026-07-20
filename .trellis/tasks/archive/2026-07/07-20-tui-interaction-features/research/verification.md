# 交互功能验证证据

日期：2026-07-20。

## 行为与实现审计

- Models、Daily、Hourly、Cost、Blocks、Stats source table 均通过同一个
  `ScrollState::visible_range` 构建窗口，并在 absolute index 等于
  `selected` 时应用 `theme::selection_style()`。
- Models、Daily、Cost、Blocks 使用稳定的引用排序，不修改已加载 payload；
  `o` 循环列，`O` 反转方向，每 panel 独立保存状态，表头显示方向箭头。
- Models/Cost 未排序时沿用世代缓存的 long-tail collapse；排序后禁用 collapse，
  `update_scroll_total` 改用原始 payload 长度。
- footer spinner 使用固定宽度 ASCII 帧，仅在 panel loading 或 sync active 时显示。
- 删除 TUI-only `trends.rs`、`sources.rs`、`projects.rs`、`health.rs` 与四个死缓存字段；
  保留仍被 Web/snapshot/测试使用的 `Dashboard::project_breakdown` 查询 API。

## 自动验证

- `cargo fmt --all -- --check`：通过。
- `cargo check --all-targets --all-features`：通过。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。
- `cargo test tui:: --lib -- --test-threads=1`：66 passed，1 个本地性能证据测试按设计 ignored。
- `cargo test --test tui_panels_prop -- --test-threads=1`：19 passed。
- `cargo test -- --test-threads=1`：退出码 0；library 365 tests，其中 2 个本地性能证据测试按设计 ignored；其余 library、integration 与 doc tests 通过。
- `npm --prefix docs run docs:build`：VitePress production build 通过。
