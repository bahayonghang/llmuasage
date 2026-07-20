# TUI 渲染效率设计

## 脏标记

事件循环先排水后台结果，再仅在 `dirty` 时 draw。键盘、鼠标、resize、dialog、数据到达、状态变化、主题变化置脏；纯 Tick 仅执行定时检查。loading/sync/spinner 活动时 Tick 置脏，空闲 Tick 不置脏。

## 主题与行数据

每帧调用一次 `theme::snapshot()`，通过 render context 引用下传，面板不得再独立取全局锁。表格以 visible range 切片，只构建窗口行；格式化 memo 以 panel data generation + sort key 为键，scroll/selection 只改变窗口。

## 事件治理

事件通道保留无界按键/鼠标/resize，Tick 使用独立容量 1 的通知或 recv 后折叠连续 Tick，确保 Tick 不积压且用户事件不丢失。

## 兼容

同状态、尺寸、主题下 TestBackend buffer 必须与 style 任务基线相同。
