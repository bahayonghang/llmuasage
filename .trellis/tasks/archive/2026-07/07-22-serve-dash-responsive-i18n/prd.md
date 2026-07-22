# serve 看板响应式与 i18n 正确性修复（S1，P0）

父任务：`.trellis/tasks/07-22-serve-dashboard-ui-perf`（全局约束 H1–H5 与产品决策 D4 继承自父 PRD）。

## Goal

消除看板在常见视口下的破版（内容溢出/按钮挤压/信息失联），让告警状态一眼可辨，统一数字与文案口径，使英文 locale 真正可用。

## Requirements

- R1.1：修复筛选栏 1101–1450px 溢出。去掉 `.filter-range-group` 的硬性 min-width（layout.css:172-174）或改为自适应列模板，并补 ~1400px 中间断点；同步补齐 explorer-controls（components.css:1253-1263，720–1100px 转 2 列）与 topbar（layout.css:26-57，中间断点允许 wrap）。(Facts A1/A5/A6)
- R1.2：给 `[data-tone]` 加容器级视觉：warn（及 error/success 如已写入）至少覆盖边框色、底色渐变、eyebrow 色；保持暗/浅双主题可读。(Fact A2)
- R1.3：同步中心 metric 数值统一走 `formatNumber`（sync-command-center.js:89-101）；KPI 脚注中的数字与主值同用等宽数字字体（components.css:754-780）。(Facts A7/B22-局部)
- R1.4：KPI 文案从 data/derive.js:447-481 移入 UI_COPY/copy.js 的 key 体系（两张卡的英文后缀风格一并统一）；locale 切换时同步 `document.documentElement.lang`。(Fact A8)
- R1.5：移动端 ≤720px 不再 `display:none` 隐藏系统健康卡（layout.css:613-615），按父任务 D4 提供紧凑/折叠的健康摘要入口；桌面端布局不变。(Fact A15)
- R1.6："应用筛选"改 `btn-primary`（shell.rs:227-230），与"重置"拉开权重。(Fact A16)

## Acceptance Criteria

- [x] A1.1：viewport 矩阵（1366×768、1440×900、1280×800、1024×768、390×844）下：文档级无横向滚动条；筛选栏/topbar/explorer-controls 容器内无溢出（脚本断言每个 rail 的 `scrollWidth <= clientWidth`）；"应用筛选/重置"按钮文字单行显示、不逐字竖排、触控目标高度 ≥32px。截图存证 output/playwright/。
- [x] A1.2：构造 `lossy_rebuild_risk` 状态截图，warn 横幅与常态有明确视觉差异（人工评审截图即可）。
- [x] A1.3：同步中心 metric 数字带千分位；KPI 脚注数字等宽。node 测试或截图断言。
- [x] A1.4：英文 locale 下四张 KPI 卡标签/脚注全英文且风格一致；`document.documentElement.lang` 随 locale 切换。node 测试覆盖。
- [x] A1.5：390×844 下健康摘要可达且不破版（截图）。
- [x] A1.6：`node --check` / `node --test`（dashboard JS）与 `just ci` 通过。

## Notes

- 仅改 `src/web/assets/{layout.css,components.css,base.css}`、`render/sync-command-center.js`、`render/hero.js`、`data/derive.js`、`copy.js`、`shell.rs` 中列出的点；不做顺手重构。
- 验证用 docs fixture（`cargo run --features testing --example docs_dashboard_serve -- --port 37421`）。

## 验收记录（2026-07-22）

- 当前源码 fixture 在 390×844、1024×768、1280×800、1366×768、1440×900 下复核；document、topbar、filter rail、explorer controls 均满足 `scrollWidth == clientWidth`，两个筛选按钮高 35px、横向单行。
- 390px 首屏健康摘要默认折叠且可达；同一会话动态放大到 1440px 后 `details.open` 自动变为 true，未留下不可见空壳。
- 英文四张 KPI 的 label/foot 全部切换，`lang=en`；中文为 `lang=zh-CN`。最终截图位于 `output/playwright/final-current-*.png`，warn 证据见 `s1-warn-*.png`。
