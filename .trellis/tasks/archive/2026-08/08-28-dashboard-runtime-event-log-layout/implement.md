# Implementation Plan

## 1. Align shell structure and navigation

- [ ] 调整 `src/web/shell.rs`，将 `#status` 从成本 grid 移为独立顶层 section，并按 `#cost` → `#status` → `#logs` 排列。
- [ ] 为运行状态补齐中英文 section 标题和说明，同时保留现有渲染目标 id。
- [ ] 调整 `src/web/assets/app.js` 的观察顺序，使其与侧栏和 DOM 一致，并确认三个 hash 的高亮行为。

## 2. Compact runtime status layout

- [ ] 调整 `src/web/assets/layout.css` / `components.css`，移除成本与状态的共享行依赖。
- [ ] 将 `.status-diagnostics-stack` 改为匹配两个子区块的平衡双列，并在 1100px/720px 断点下可靠降级。
- [ ] 检查诊断卡片、失败记录、长文案、light/dark 与中英文排版，避免空列、极窄列和页面溢出。

## 3. Bound and refine event logs

- [ ] 将 `src/web/assets/data/fetch.js` 的日志展示页大小改为 20，并保留 filter、cursor、session、event-key 参数。
- [ ] 更新 `src/web/assets/render/logs-viewer.js`，复用现有日期 formatter，添加语义列 class、长文本 title/ellipsis、键盘展开和 `aria-expanded`。
- [ ] 更新 `src/web/assets/components.css`，实现日志局部双向滚动、sticky header、稳定列宽、行 focus/expanded/raw detail 层级与窄屏触控表现。
- [ ] 保留 snapshot 空态、reset、stale generation、load-more append 和 raw detail lazy fetch 行为。

## 4. Regressions

- [ ] 在 `src/web/mod.rs` 增加 embedded shell/assets 结构与顺序断言。
- [ ] 扩展 `scripts/tests/dashboard-fetch.test.mjs`，固定日志首批 20 条及 cursor/detail 参数。
- [ ] 扩展 `scripts/tests/dashboard-logs-viewer.test.mjs` 或现有已纳入门禁的 Dashboard 测试，覆盖新增纯行为；确保相关测试实际进入 `just ci`。
- [ ] 检查新增中英文 key、模块 import 和 asset manifest 一致性。

## 5. Validation

- [ ] 对修改的 JavaScript 运行 `node --check`。
- [ ] 运行聚焦 Node logs/fetch tests。
- [ ] 运行 `cargo fmt --check` 和 `cargo test web::tests -- --test-threads=1`。
- [ ] 使用 `http://127.0.0.1:37421/` 验证 `#cost`、`#status`、`#logs` 的 hash、标题和高亮；验证 2048×1120、1440×900、390×844 的布局与无页面溢出。
- [ ] 运行完整 `just ci`；若环境仍只有 `python3`，使用临时 PATH 兼容层原样重跑，不修改仓库 recipe。
- [ ] 运行 `git diff --check`、`git diff --stat` 并审查最终 diff 只包含本任务范围。

## Risk Points

- 只移动 markup 而不统一 observer 顺序会保留 hash/高亮漂移。
- 只把 50 改为 20 而不限制表格 viewport，日志页仍可能过长；只限制高度而不提供 sticky header/overscroll 会降低可用性。
- sticky header 的背景与层级若使用错误变量，会在 light theme 下透出正文。
- `title`、ellipsis 或固定列宽不能施加到 `<td>` 的所有内容上，否则数值和 agent tag 可能失去可读性。
- 键盘展开必须阻止 Space 的页面滚动，并让 `aria-expanded` 与 detail row hidden 状态同步。
