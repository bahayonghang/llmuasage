# TUI 首访渲染线程阻塞基准设计

## 测量边界

基准位于 `src/tui/mod.rs` 的测试模块，复用真实 `PanelDataLoader`、`AppState`、
`request_panel_data`、结果应用逻辑和 `draw::draw`。TestBackend 固定为 120x30，
避免依赖交互终端，同时执行与生产相同的渲染函数。

一次访问拆成四个互不重叠的同步区段：请求分发、loading 帧绘制、匹配结果应用、
populated 帧绘制。结果未到达时只轮询异步通道并 yield/sleep，该等待不进入任何
计时。每次访问取四段最大值，面板指标取预热后 3 次最大值的中位数。

## 最小代码调整

将 `apply_panel_results` 中单个匹配结果的状态更新提取为私有
`apply_panel_result`，生产循环仍以同样顺序排水、过滤和更新。基准先在计时区外
取到 `PanelResult`，再单独计时该 helper，从而不把后台等待混入状态应用时长。

## 安全性

通过 `AppPaths::discover` 只读使用本地代表性数据库；测试不调用 bootstrap、sync
或写接口。测试标记 ignored，仅显式执行。无新增依赖、schema 或生产行为变化。
