# TUI 样式统一与主题体系扩展

父任务：`.trellis/tasks/07-20-tui-optimization`。证据：父任务 `research/llmusage-tui-analysis.md` §4、`research/tokscale-patterns.md` §5/§6（模式 12/13）。建议顺序：在 07-20-tui-render-efficiency 之前串行进行（两者同触 theme.rs 与全部面板，本任务先定 golden 基线）；经 design 确认改动面不重叠时可与 07-20-tui-time-window-bounding 并行。

## Goal

视觉一致性：所有颜色经主题槽位（切主题全界面跟随）、文案语言统一、数字格式化单一实现、修正误导性提示；主题从 2 套扩充并支持受限终端降级。

## Confirmed Facts

- 硬编码颜色绕过主题：overview.rs（metric_line 全部）、daily.rs、hourly.rs、stats.rs、usage.rs、behavior.rs、health.rs 直接用 `Color::{Green,Cyan,Yellow,Magenta,Blue,Red}` → 切主题只重着色部分界面。theme.rs 已有语义槽位体系（accent/muted/kpi_colors[4]/heat[5]/bar_ok|warn|danger/trend_*）与 2 套主题（default_dark 须保持像素级默认外观、catppuccin_mocha）。
- 文案中英混杂：面板标题/占位（概览/模型/成本/… vs Daily Usage/Stats/…；"加载中…" vs "Loading…"）；footer/help/overview 标签混用。
- `format_number`（千分位）在约 11 个文件重复实现；另有 `format_tokens`(k/M)、report_table `format_token_compact`(K/M/B)、成本 "${:.2}"/"{:.4}" 各处分散。
- footer.rs:52 写 "tab/shift-tab or 1-8 view"，实际 9 个面板（help_dialog.rs:57 正确写 1-9）。
- 5 个表格面板设置了 `row_highlight_style` 但用 `render_widget` 无 TableState，样式永不生效（models.rs:111、cost.rs:114、sources.rs:102、projects.rs:83、blocks.rs:113）——本任务只清理死样式或留给 interaction 任务接线（与其协调，不重复处理）。
- tokscale 对标：12 套主题（5 级强度 ramp + 语义槽位）；`TerminalColorMode` 按 TERM/COLORTERM/NO_COLOR 环境降级 RGB→16 色 ANSI（themes.rs:290-307,366-403）；指标色约定 input=绿/output=红/cacheR=蓝/cacheW=橙。
- 仓库既有无色约定（2026-07-20 评审补充）：CLI 侧 `ColorMode::from_env`（src/tui/report_table.rs:318-327）把 NO_COLOR/LLMUSAGE_NO_COLOR 解释为 `Never`（完全无色），**不是**降级到 16 色。TUI 当前不读 NO_COLOR；引入无色支持时必须与该约定一致，不能照抄 tokscale 的「NO_COLOR→ANSI16」语义。
- 默认主题回归红线：theme.rs 注释与测试（theme.rs:343-352）要求 default_dark 与历史外观逐值一致。

## Requirements

- R1 主题槽位全覆盖：清点全部硬编码 `Color::*` 调用点，映射/扩充语义槽位（如需新增 metric 槽位——input/output/cache 读写四色，对齐 tokscale 约定），替换后默认主题渲染输出保持不变（TestBackend buffer 等值）。
- R2 文案统一：确定语言策略（建议界面文案统一英文、`README.zh-CN` 生态不受影响；最终决策 design 阶段与用户确认），一次性统一面板标题/占位/footer/help；footer "1-8"→"1-9" 之类事实性错误一并修正。文案变更必须独立成阶段（见 R6 分步验收）并输出逐条变更清单（旧文案 → 新文案）。
- R3 共享格式化：新建 `tui` 层共享格式化模块（千分位/token 紧凑/成本/百分比），全部面板与 report_table 改为引用；各处现有输出格式保持逐字节不变（有意统一的除外，需在 prd 列明差异清单）。
- R4 主题扩充与终端适配（两个独立机制，不得混同）：(a) 无色模式——NO_COLOR/LLMUSAGE_NO_COLOR 下 TUI styling 全关，对齐 report_table `ColorMode::Never` 既有约定；(b) 受限色终端——非 truecolor 的 TERM/COLORTERM 下 RGB→16 色 ANSI 降级（对标 tokscale compatible_rgb）。另新增至少 2 套主题（候选：tokscale Graphite/Lagoon 类 surface 主题）。默认全彩路径零变化。
- R5 一致性守护：为「无硬编码颜色」建立守护（clippy lint 不可行则 grep 检查脚本或单测扫描源码），防止回归。
- R6 分步验收与测试：本任务必须拆为两个可独立回退的阶段——阶段一（颜色槽位化 + 共享格式化迁移，纯重构）：默认主题 TestBackend buffer 与现状逐字节等值；阶段二（文案统一）：相对阶段一基线仅文本 cell 变化且逐条对应 R2 变更清单，样式属性（fg/bg/modifier）不变。另有无色/受限色映射单测、主题循环测试更新。

## Acceptance Criteria

- [ ] A1：`t` 循环切换任一主题，9 个面板+nav+footer+对话框颜色全部跟随（人工核验记录 + R5 守护通过）。
- [ ] A2：阶段一完成点：默认主题渲染输出与现状逐字节一致（TestBackend 证据）。阶段二完成点：与阶段一基线相比仅文本 cell 差异，且每处差异对应 R2 变更清单条目（样式属性零变化）。
- [ ] A3：`format_number` 等重复实现清零（单一模块引用）；文案语言统一，无中英混杂残留清单。
- [ ] A4：NO_COLOR/LLMUSAGE_NO_COLOR 下 TUI 无任何 styling（对齐 `ColorMode::Never` 约定，测试证据）；受限色终端下 RGB→ANSI16 降级可读；新主题可用。
- [ ] A5：`cargo test -- --test-threads=1`、fmt、严格 clippy 全绿。

## Out Of Scope

- 行选中/排序等交互（07-20-tui-interaction-features；row_highlight_style 死样式的接线归其管）。
- report_table CLI 输出的配色改版（仅允许共享格式化函数替换，输出不变）。
- web dashboard 样式。

## Notes

- 复杂任务：`task.py start` 前必须补 `design.md`（语言策略决策、槽位映射清单、两阶段拆分）与 `implement.md`。
