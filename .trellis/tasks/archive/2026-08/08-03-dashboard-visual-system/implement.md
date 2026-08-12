# Implement：看板视觉系统对齐 AgentsView

前置阅读：本任务 `prd.md`、`design.md`；父任务 `research/agentsview-inventory.md` §3–4、`research/llmusage-web-current-state.md` §3、§5。

> ⚠️ 所有 CSS/JS 修改通过 Bash（heredoc / python 脚本 / sed）完成，禁用 Edit/Write 工具直改 —— 全局 prettier hook 会破坏仓库风格并打断 CI 测试。shell.rs 是 Rust 文件可用 Edit。

## 步骤

### 1. Token 原语层替换（`src/web/assets/base.css`）

- [ ] 移除 `--ctp-*` 原语，写入 AgentsView light（`:root`）/ dark（现有主题选择器）原语，值精确取自父任务 research §4。
- [ ] 建立别名：`--bg/--surface/--surface-2/--ink/--ink-2/--ink-strong/--muted/--line/--accent/--data-accent/--good/--warn/--danger` → 新原语。
- [ ] 新增 `--radius-sm/md/lg`、三档 shadow、`--transition-fast`、7 个 `--source-<id>` 身份色。
- [ ] root 字号 13px、Inter/JetBrains Mono 前置字体栈。
- [ ] `grep -rn 'ctp-' src/web/assets/` 清零残留。
- 验证：`cargo run -- serve` 目测双主题；`node --check` 无涉及可跳过。

### 2. 组件规格定点修改（`components.css`、`charts.css`）

- [ ] KPI 卡规格（12px padding / 20px 值 / 11px 标签 / featured 2px 蓝边，去渐变）。
- [ ] 源徽章：渲染器（`render/sources.js` 等源标签输出处）追加 `data-source` 属性（Bash 编辑）；CSS `[data-source]` 选择器接 `--source-<id>` 着色 + 徽章规格。
- [ ] 下游契约 token 定义：`--source-<id>` ×7、`--chart-cat-1..6`/`--chart-cat-other`、`--hm-l0..l4`（light/dark 双份，进 `base.css`/`charts.css`）。
- [ ] 条形图蓝色化 + nav pill 激活态 + 浮层 chrome + `.chart-tooltip` 预置类。
- 验证：双主题 + zh/en 四组合截图抽查；七源徽章着色逐一核对；`grep -c 'source-\|chart-cat-\|hm-l'` 确认契约 token 双主题齐全；键盘 Tab 走查 focus 环。

### 3. 版面网格（`layout.css` + `src/web/shell.rs`）

- [ ] `.dash-grid` / `.wide` / 760px 断点；Distribution 与 Cost 区面板挂网格；KPI 行 flex-wrap 化。
- [ ] shell.rs 容器类名同步（注意 format! 的 `{{}}` 转义）。
- 验证：`cargo test -- --test-threads=1`（shell 相关测试）；1440/1100/720 三宽度目测。

### 4. 全量验证

- [ ] `just ci`。
- [ ] `cargo run -- export html -o /tmp/snap.html`（或项目实际导出命令）打开验证快照模式样式。
- [ ] 检查 `docs/dashboard/index.md` 是否有配色描述需微调（截图重拍留给父任务集成审查）。

## 回滚点

- 步骤 1 独立提交（纯 token）；步骤 2、3 各一提交。任一步出问题 `git revert` 单提交即可，别名层保证向后兼容。

## 提交建议

Conventional Commits + 中文 scope，如 `style(看板): [AI] 🎨 设计 token 迁移至 AgentsView 色板`。
