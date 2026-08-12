# Design：看板视觉系统对齐 AgentsView

## 边界

主体是 `src/web/assets/` 下 CSS（`base.css`、`layout.css`、`components.css`、`charts.css`）与少量 shell 结构类名（`src/web/shell.rs`）。**例外**（review-verification §9）：源标签着色需要 markup 携带源标识，允许对渲染器（`render/sources.js`、`render/costs.js` 等输出源标签处）做最小改动 —— 仅追加 `data-source="<id>"` 属性，不动数据流、导出形状、后端、i18n 键。

## Token 迁移策略：别名层，不做全量替换

`base.css` 现状是 `--ctp-*` 原语 → 语义 token（`--bg/--surface/--ink/--accent/--line/...`）两层。迁移方案：

1. 删除/停用 `--ctp-*` 原语层，新增 AgentsView 原语层（`--bg-primary/--bg-surface/--bg-inset/--border-default/--text-primary/--accent-blue/...`，light 在 `:root`，dark 在 `[data-theme='dark']` —— 沿用现有主题属性机制，不改成 `.dark` 类）。
2. 既有语义 token 保留名字、改指新原语：`--bg→--bg-primary`、`--surface→--bg-surface`、`--surface-2→--bg-inset`、`--ink→--text-primary`、`--ink-2→--text-secondary`、`--muted→--text-muted`、`--line→--border-default`、`--accent→--accent-blue`、`--data-accent→--accent-blue`、`--good/--warn/--danger→--accent-green/amber/red`。
3. 消费方（2433 行 components.css 等）基本零改动 —— 只在样式规格与 AgentsView 有差异处定点改（卡片 padding、字号、半径、徽章、tooltip、条形）。
4. **下游契约 token 一并在本任务定义**（prd R2 命名即契约）：`--source-<id>` ×7、`--chart-cat-1..6`/`--chart-cat-other`、`--hm-l0..l4`，light/dark 双份。ready-widgets 与 session-analytics 直接引用这些名字，不再各自定义。

理由：全量重命名会触碰全部 4 个 CSS 文件的每一行并放大 review 面；别名层把色彩迁移和组件规格迁移解耦，可分两次提交回滚。

现有 `color-mix(in oklab, ...)` 派生保留（AgentsView 用 `in srgb`，视觉差异可忽略；仅激活态等对齐处按规格用 srgb）。

## 主题切换机制

保留现有 `theme.js` + `localStorage['llmusage:theme']` + shell 头部预绘制脚本；只替换选择器命中的变量值。不引入 AgentsView 的 `.dark` on `:root` 约定。

## 关键组件规格 diff（components.css 定点修改清单）

| 组件           | 现状                  | 目标                                                                                                                                                                                                                  |
| -------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `.kpi` 卡      | 项目自有 padding/字号 | `padding:12px; min-width:120px`；`.kpi-value` 20px/600；`.kpi-label` 11px/500 `--text-muted`；`.kpi-sub` 10px；featured 卡 2px `--accent-blue` 边框（替换现有渐变 `--kpi-featured-grad`）                             |
| 源徽章 | `.src-name` 纯文本（无源属性） | 保留 `.src-name` 类名，渲染器追加 `data-source` 属性；CSS 按 `[data-source]` 匹配 `--source-<id>` token 着色；徽章规格 8px/600 大写、`letter-spacing:.02em`、`opacity:.7`；七源映射 claude blue/codex green/opencode purple/kimi_code pink/pi indigo/antigravity violet/grok muted |
| tooltip        | SVG `<title>`         | 保留 `<title>`（零 JS 改动），另为后续部件预置 `.chart-tooltip` 反色 chip 类（本任务只定义样式，不接 JS）                                                                                                             |
| 条形           | peach `--data-accent` | `--accent-blue` 填充 + `--bg-inset` 轨道 + 2px 圆角 + 14px 高                                                                                                                                                         |
| nav 链接激活态 | 项目自有              | pill：12% blue 底 + blue 字 + 600                                                                                                                                                                                     |
| 浮层           | 项目自有              | `--bg-surface + 1px --border-default + --radius-md + --shadow-lg`                                                                                                                                                     |

## 版面网格

`layout.css` 新增 `.dash-grid { display:grid; grid-template-columns:1fr 1fr; gap:12px }`、`.dash-grid .wide { grid-column:1/-1 }`、`@media (max-width:760px){ .dash-grid{grid-template-columns:1fr} }`。本任务把 Distribution 三面板与 Cost 区面板挂进该骨架；Overview KPI 行改为 flex-wrap 统计卡行。shell.rs 相应容器类名调整（Rust format 字符串内，注意 `{{` 转义）。

## 兼容与回滚

- 每步保持 `just ci` 可过；CSS-only 提交与 shell 结构提交分开。
- 回滚单位 = 单文件 git revert；token 别名层保证旧类名依旧解析。

## 风险

1. `components.css` 中直接引用 `--ctp-*` 的散点（若有）需 grep 清零，否则 dark 模式出现死色。
2. shell.rs 是 format! 字符串，类名改动需跑 `cargo test`（有 shell 快照类测试则更新）。
3. 对比度：AgentsView muted 色在 13px 下已验证，但 llmusage 保留的中文文案更密，验收时抽查 zh 界面可读性。
