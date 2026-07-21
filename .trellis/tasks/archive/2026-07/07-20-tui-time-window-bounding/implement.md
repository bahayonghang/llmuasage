# TUI 时间窗与扫描范围执行计划

- [x] 先写窗口到本地日期 filter 的时区/边界测试，默认 All 回归测试。
- [x] 把窗口快照接入异步请求，仅失效受管辖面板。
- [x] 逐查询传递有界 filter，保留 lifetime/fixed-window 例外。
- [x] 更新 footer/nav/help 的窗口标签与范围说明。
- [x] 实现 Blocks 断档探测和等值测试：跨边界、断档重锚、active、无断档回退。
- [x] 记录 release 3 次中位数耗时与扫描行数。
- [x] 运行 focused tests、fmt、严格 clippy、串行全测。

回滚：窗口接线与 Blocks 收敛分别提交；Blocks 证据不满足等值时保留异步全量实现并将任务保持未完成。
