# Implementation Plan

## 1. Lock status semantics

- [x] 更新 `src/web/assets/render/hero.js`，让 hero 消费 `syncCommandCenter` 的 tone、headline、last run 与 source readiness。
- [x] 更新 `src/web/assets/data/render-key.js`，把 `sync_command_center` 纳入 hero 指纹。
- [x] 在 `src/web/assets/copy.js` 补齐中英文 quick-status 文案与结构化 status 映射。
- [x] 保留 `health/diagnostics` 的历史失败投影，不修改 Rust 查询和数据库记录。

## 2. Refine sidebar footer

- [x] 调整 `src/web/shell.rs` 的 footer 语义结构，同时保留现有控件 id 和 i18n 属性。
- [x] 在 `src/web/assets/components.css` 与必要的响应式规则中统一偏好控件、endpoint 卡片、focus/hover 和窄屏表现。
- [x] 收敛 `src/web/assets/app.js` 的 endpoint 写入职责，移除 job summary、job id 和原始错误文本对 `#endpoint-sync` 的覆盖。

## 3. Document and regress

- [x] 更新 `docs/dashboard/index.md`，说明右上角显示当前同步健康，历史中断保留在诊断详情。
- [x] 更新 `src/web/mod.rs` 的 embedded shell/assets 回归断言。
- [x] 如纯模块行为需要更精确覆盖，在 `scripts/tests/dashboard-render-lifecycle.test.mjs` 增加最小指纹测试。

## 4. Validation

- [x] `node --check` 检查所有改动的 JavaScript 文件。
- [x] `node --test scripts/tests/dashboard-render-lifecycle.test.mjs`。
- [x] `cargo test web::tests -- --test-threads=1`。
- [x] `just ci`。
- [x] 本地浏览器验证桌面宽屏与 720px 以下布局；确认当前本机两条 `serve aborted` 不再触发右上角警告，并确认历史诊断仍存在。
- [x] `git diff --check`、`git diff --stat` 与最终 diff 范围审查。

## Risk Points

- hero 指纹若遗漏 `sync_command_center`，状态会在后台变化后停留旧值。
- 直接复用自由文本 headline 作为 pill 会使桌面卡片变宽；pill 必须保持短文案。
- 删除所有 endpoint 错误写入后，必须确认加载失败仍由既有 hero/bootstrap error surface 明确展示。
- CSS 修改必须同时检查 light/dark 变量与 720px 断点，避免固定色值和移动导航增高。
