# TUI 异步面板加载执行计划

- [ ] 先加 generation/filter 匹配、冷 loading 首帧、退出取消测试。
- [ ] 实现 loader 请求/结果/取消句柄和有界通道。
- [ ] 逐面板迁移 Overview、sync、Models、Daily、Hourly、Cost、Blocks。
- [ ] 并行化 Stats/Behavior 子载荷并加入串行等值测试。
- [ ] 接入 refresh/source/window 失效与 stale-while-refresh 状态。
- [ ] 记录代表性快照的串行/并行 release 3 次中位数。
- [ ] 运行 focused tests、fmt、严格 clippy、串行全测。

回滚：基础 loader 与逐面板迁移分阶段提交；任一面板可回退到最近已验证阶段，但任务完成时不得保留 UI 线程查询。
