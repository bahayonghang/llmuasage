# TUI 交互增强执行计划

- [x] 先扩展 ScrollState 属性测试并实现选中/窗口动作。
- [x] 接入表格选中样式、PgUp/PgDn/Home/End 与滚轮。
- [x] 实现每面板排序状态、稳定排序和表头指示测试。
- [x] 接线 fixed-width spinner 与后台活动状态。
- [x] 删除死字段/模块；评估并接入或删除 dead trends bar。
- [x] 更新 help/footer/docs 键位说明。
- [x] 运行 focused tests、fmt、严格 clippy、串行全测。

回滚：状态机、排序、spinner、死代码分别形成验证点；不得恢复无状态 row highlight 或重复窗口机制。
