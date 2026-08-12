# 看板首页宽屏空白布局优化

## Goal

消除本地用量概览页在宽屏下出现的无意义空白区域，让 hero 状态卡与会话分析面板在可用主栏宽度内贴合内容，避免“像缺了组件”的空洞感。

## Problem

截图显示两处明显空白：

1. **右上角（hero 区）**：标题/系统健康卡右侧大片空区。
2. **热门会话右侧**：`dash-grid` 两列布局下，`top-sessions` 独占左列，右列无内容。

## Root Cause（已核实）

1. `.hero` 使用固定上限列宽 `minmax(0, 640px) 360px`，主栏更宽时网格不拉伸，右侧留白。
2. `#top-sessions` 未标记 `wide`，在 `ready-widgets-grid.dash-grid` 中处于两个 `wide` 面板之间，单独占半宽。
3. `.status-grid` 写死 3 列，但 hero 只渲染 2 个 `status-cell`，状态卡内部也会多出一格空洞。

## Requirements

- 宽屏（主栏 > ~1000px）下，hero 标题区占满剩余空间，系统健康卡靠右贴齐主栏内容区右缘，右侧不再出现大片空白。
- `热门会话` 在当前没有并排伙伴面板时，应占满一行（与活动日历、每日 Token 构成同级全宽）。
- 系统健康卡内的 metric 格与真实 cell 数量一致，不预留空 cell。
- 现有窄屏/中等断点行为不回归：≤1100px hero 仍可单列；≤760px `dash-grid` 仍单列。
- 不改变数据契约、API 或文案语义；仅布局/结构类修复。

## Acceptance Criteria

- [x] 宽屏桌面视口下，hero 右侧不再出现与 status-panel 同级的空白列区域。
- [x] 宽屏桌面视口下，热门会话面板横向占满 `ready-widgets-grid` 内容宽。
- [x] 系统健康卡只显示 2 个 metric cell 时，不出现第三格空白槽位。
- [x] 相关 shell/CSS 契约有回归断言（Rust asset/shell 测试或等价检查）。
- [x] 本地浏览器或等价手段确认 overview 页视觉正确，且 ≤1100 / ≤760 断点无布局崩溃。

## Out of Scope

- 新增第三块 status metric 或新的并排会话分析面板。
- 汇总数据为空（“当前范围暂无汇总数据”）属于数据范围问题，不在本任务修复。
- 同步失败 banner / antigravity 风险源提示的产品逻辑。

## Notes

- 轻量布局修复：PRD-only。
- 验证优先对照用户截图的两个红框区域。
