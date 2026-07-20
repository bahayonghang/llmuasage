# TUI 交互增强设计

## 选中、窗口与排序

扩展现有 `ScrollState` 为 selected/offset/visible/total，统一负责窗口锚定。j/k、上下、PgUp/PgDn、Home/End 移动选中；滚轮复用同一动作。Models、Daily、Cost、Blocks 的排序键统一为 `o` 循环列、`O` 反向，避免占用 `t` 主题键；每 panel 保存 sort key/direction，排序稳定并在表头显示箭头。

## 加载与鼠标

footer 在 async loading、refreshing、sync running/cancelling 时用 `spinner_frame` 渲染固定宽度 spinner，避免布局抖动。鼠标左键 nav 不变，滚轮作用当前表格。

## 死代码决策

删除未分派的 `sources.rs`、`projects.rs`、`health.rs` 及 AppState 的 trends/sources/projects/health 死缓存。保留 `Dashboard::project_breakdown`（web 使用）。将 `trends.rs` 的高质量 bar 算法并入 Hourly 后删除原死模块；若等值接入不能在本任务证明，则删除死模块并保留当前 Hourly，避免两套未维护实现。

## 文档与兼容

help/footer 和现有 dash reference 同步键位；排序仅改变已加载集合顺序，不发起查询。
