# PRD：看板视觉系统对齐 AgentsView

父任务：`.trellis/tasks/08-03-dashboard-agentsview-alignment`（研究产物在父任务 `research/` 下，含审阅核验事实 `review-verification.md`）。

## Goal

将 llmusage 网页看板的设计 token、排版与组件样式从 Catppuccin 体系迁移到 AgentsView 的中性灰蓝 + 单蓝主色体系，使整体观感与 `ref/repo/agentsview` 一致，同时不改变信息架构与任何功能行为。

## Requirements

### R1 设计 token 替换（`base.css`）

以 AgentsView token 值重写语义 token 层（精确值见父任务 `research/agentsview-inventory.md` §4）：

- Light：`--bg-primary #f5f6f8`、`--bg-surface #ffffff`、`--bg-surface-hover #f0f1f4`、`--bg-inset #ecedf2`、`--border-default #d8dae2`、`--border-muted #e4e6ec`、`--text-primary #181b24`、`--text-secondary #555b6e`、`--text-muted #878ea0`、`--accent-blue #2563eb`。
- Dark：`--bg-primary #0d0d12`、`--bg-surface #16161e`、`--bg-surface-hover #1f1f2a`、`--bg-inset #111116`、`--border-default #333345`、`--border-muted #2a2a3a`、`--text-primary #ececf1`、`--text-secondary #b4bcd0`、`--text-muted #8b93a8`、`--accent-blue #60a5fa`。
- 语义辅助色六件套（green/amber/red/purple/teal + blue）双主题；`--radius-sm/md/lg: 4/6/8px`；三档阴影；`--transition-fast .12s`；`--focus-ring: 2px solid var(--accent-blue)`。
- 现有语义 token 名（`--bg/--surface/--ink/--accent/--line` 等）保留为别名指向新值；`--data-accent` 改指 `--accent-blue`。

### R2 下游任务引用的 token 名（本任务定义，命名即契约）

后续子任务（ready-widgets、session-analytics）按以下名字消费，本任务在 `base.css`/`charts.css` 中定义双主题值：

- 源身份色：`--source-claude`（blue）、`--source-codex`（green）、`--source-opencode`（purple）、`--source-kimi-code`（pink）、`--source-pi`（indigo）、`--source-antigravity`（violet）、`--source-grok`（muted）。
- 分类图表色：`--chart-cat-1..6` + `--chart-cat-other`（值 = AgentsView 六色板，light/dark 双份，见 research §4）。
- 热力标尺：`--hm-l0..l4`（l0 = `var(--bg-inset)`，l1–l4 = GitHub 绿，light/dark 双份）。

### R3 排版

- root 字号 13px；字体栈 `"Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", ...`（仅本地栈，不下载 webfont）；mono 栈 `"JetBrains Mono", "SF Mono", Menlo, Consolas` 前置。
- 数字保留 `tabular-nums`。

### R4 组件样式

- 统计卡对齐 AgentsView 规格：`flex; gap:8px`，卡 `min-width:120px; padding:12px`，值 20px/600，标签 11px/500 muted，副标签 10px；featured 卡 2px `--accent-blue` 边。
- 源徽章：8px 大写有色文字（`opacity:.7`，非填充胶囊）。**边界修正**（review-verification §9）：现有渲染器输出 `.src-name` 且 markup 无源标识属性，纯 CSS 无法按源着色 —— 允许对渲染器做**最小 markup 改动**（为源标签元素追加 `data-source="<id>"` 属性），CSS 用 `[data-source='claude']` 等选择器接 `--source-*` token。不改任何数据流/导出形状。
- 图表 tooltip 反色 chip：`background: var(--text-primary); color: var(--bg-primary); padding:4px 8px; font-size:10px; radius-sm`。
- 导航 pill 激活态：`color-mix(in srgb, var(--accent-blue) 12%, transparent)` 底 + `--accent-blue` 字 + 600 字重。
- 条形图（models/sources/projects/costs）统一 `--accent-blue` 填充、`--bg-inset` 轨道、`radius 2px`、高 14px。
- 浮层统一 `--bg-surface + 1px --border-default + --radius-md + --shadow-lg`。

### R5 版面

- 看板主体区 `1fr 1fr` gap 12px 网格骨架、`.wide` 通栏类，≤760px 塌单列；现有面板归位到该网格。
- 侧栏与顶部结构保留，间距、边框、hover 态用新 token。

### R6 不变式

- zh/en 字符串体系、light/dark 切换机制（`data-theme` 属性 + localStorage）、`prefers-reduced-motion`、`:focus-visible`、aria 属性、降级 chip 体系、快照导出模式全部不变。

## 约束

- CSS/JS 编辑必须走 Bash（全局 prettier hook 会破坏仓库单引号风格与 CI）。
- 渲染器 markup 改动仅限追加属性/类名，不改 `data/fetch.js`、`load-state.js`、渲染生命周期模块的导出形状（CI node --test 直接 import）。
- 不新增资产文件时无需动 `ASSET_MANIFEST`；若拆分新 CSS 文件必须登记 `src/web/assets/mod.rs:70` 并更新 `[WebAsset; 26]` 计数。
- 顺序依赖：本任务是子任务序列第一个，无前置。

## Acceptance Criteria

- [ ] light/dark 双主题下所有 section 视觉抽查与 AgentsView token 规格一致（对照父任务 research §4）。
- [ ] R2 全部 token 名在双主题下有定义（`grep` 可验证），供后续任务直接引用。
- [ ] 源徽章按 `data-source` 正确着色（七个注册源逐一验证）。
- [ ] 功能零回归：过滤、同步、Explorer、导出、i18n、主题切换全部照常。
- [ ] `just ci` 全绿（重点：4 个 dashboard node 测试 + clippy）。
- [ ] `export html` 产物打开后样式正确。
- [ ] `docs/dashboard/index.md` 若配色描述过时则同步（截图重拍在父任务集成审查）。
