# TUI 交互功能增强（行选中/排序/滚轮/死面板处置）

父任务：`.trellis/tasks/07-20-tui-optimization`。证据：父任务 `research/llmusage-tui-analysis.md` §3/§4/§5、`research/tokscale-patterns.md` §4/§6（模式 9/10/14）。建议最后进行（依赖 async 基座的世代/loading、style 的 selection 槽位与文案基线、render 的行窗口化状态）。

## Goal

把 TUI 从「只读滚动」升级到 tokscale 交互基线：表格行选中与高亮、列排序切换、鼠标滚轮、后台加载可视化；同时处置分析发现的死面板/死字段（接线或删除，二选一显式决策）。

## Confirmed Facts

- 现状滚动仅 offset 平移（app.rs:177-194），无选中行概念；5 个面板的 `row_highlight_style` 因无 TableState 从不生效（models.rs:111、cost.rs:114、sources.rs:102、projects.rs:83、blocks.rs:113）。
- 排序固定为 SQL ORDER BY，无用户切换；无鼠标滚轮（鼠标捕获已开但只处理 nav 左键点击 mod.rs:112-118）。
- `spinner_frame` 在 on_tick 自增（app.rs:405）但从未渲染——async/sync 后台化落地后正好用作加载指示。
- 死代码清单（处置对象）：panels/trends.rs（完整柱状图实现，含轴/峰值/紧凑回退，质量高于在用 hourly 条形）、panels/sources.rs、panels/projects.rs、panels/health.rs 四个未接线模块；AppState 死缓存字段 `trends/sources/projects/health`（app.rs:236-243）；`project_breakdown()` 查询（query/mod.rs:1133）仅 TUI 侧未调用——web `/api/projects` 与全量快照在用，函数本身必须保留。
- tokscale 对标：↑/↓ 选中（wrap）+ PgUp/PgDn/Home/End；`c`/`t`/`d` 排序键重按切换方向、每 tab 记忆（app.rs:1641）、表头 ▲/▼ 指示（widgets.rs:101-110）；滚轮移动选中（app.rs:1492-1527）；手动行窗口化渲染选中背景（models.rs:144,227）；右侧滚动条指示（widgets.rs:91）。
- 面板枚举名与展示内容错位（Panel::Sources 显示 "Daily" 等）——重命名会波及 proptest/nav 测试，属可选清理。

## Requirements

- R1 行选中：表格面板（Models/Daily/Hourly/Cost/Blocks/Stats source 表）引入 selected_index + 窗口滚动（复用/扩展 ScrollState；实现方式对标 tokscale 手动窗口化，与 render-efficiency 任务的行窗口化协同，避免两套机制）；↑/↓/j/k 移动选中（替代纯 offset 滚动），PgUp/PgDn/Home/End 支持；选中行高亮走主题 selection 槽位（新增槽位与 style-unify 协调）。
- R2 列排序：为 Models/Daily/Cost/Blocks 提供排序键（建议对标 `c` cost/`t` tokens/`d` date，重按反向；具体键位避让现有 t=theme——design 阶段定并更新 help/footer）；每面板记忆排序态；表头 ▲/▼ 指示；排序在已加载载荷上进行（不重查库）。
- R3 鼠标滚轮：滚轮上下移动选中/滚动当前面板；nav 点击行为保留。
- R4 加载指示：接线 spinner_frame——后台加载/刷新/sync 进行中在 footer 或面板标题区渲染 spinner（帧率随 tick）；与 async 任务的 loading 态、sync 任务的进度文案协同（不重复实现）。
- R5 死代码处置（显式决策，design 阶段确认）：可删对象仅限 TUI 侧——panels/sources.rs、panels/projects.rs、panels/health.rs、AppState 死缓存字段（trends/sources/projects/health）。`Dashboard::project_breakdown` 必须保留：web `/api/projects`（src/web/mod.rs:250-259）与全量快照（src/query/mod.rs:2372）在用，仅可移除 TUI 对它的死引用（如有）。panels/trends.rs 的柱状图实现评估并入 Hourly/Daily 视图（其渲染质量更高）或删除。决策与理由记录入 design.md；删除项在同一提交内保持 `cargo test` 全绿。
- R6 帮助/文档：help 对话框与 footer 快捷键提示同步新增键位；docs/reference 中 dash 键位表更新（若存在）。
- R7 测试：选中/窗口滚动边界 proptest（扩展现有 ScrollState 属性测试）；排序稳定性与方向切换单测；滚轮事件→动作映射单测；spinner 渲染 TestBackend 断言；死代码删除后全量测试绿。

## Acceptance Criteria

- [x] A1：表格面板可见选中行高亮，↑/↓/PgUp/PgDn/Home/End/滚轮行为正确且不越界（proptest 证据）。
- [x] A2：排序键生效、方向可切、表头有指示、每面板记忆；数字与排序前集合一致（仅顺序变化）。
- [x] A3：后台活动期间 spinner 可见且动画流畅；无后台活动时不渲染。
- [x] A4：死代码处置完成（决策记录 + 代码落地），无未接线模块/字段残留或已全部接线。
- [x] A5：help/footer 提示与实际键位一致；`cargo test -- --test-threads=1`、fmt、严格 clippy 全绿。

## Out Of Scope

- 搜索、日期选择器、CSV 导出、剪贴板复制（tokscale 有但本轮不引入；如需另立任务）。
- 会话级 drill-down（tokscale Daily→明细）——数据层需新查询，另立任务。
- 设置持久化文件（tokscale settings.json 模式）——本轮主题仍走 LLMUSAGE_THEME 环境变量。

## Notes

- 复杂任务：`task.py start` 前必须补 `design.md`（排序键位避让方案、R5 处置决策、与 render 窗口化机制的协同）与 `implement.md`。
