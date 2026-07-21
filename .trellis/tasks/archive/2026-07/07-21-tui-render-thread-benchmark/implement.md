# TUI 首访渲染线程阻塞基准执行计划

- [x] 提取单结果应用 helper，并保持生产排水行为不变。
- [x] 增加 ignored async release benchmark，覆盖 Stats、Behavior、Blocks。
- [x] 预热后采集每个面板 3 次四区段样本、最大值和中位数。
- [x] 将精确证据写入父任务性能基线和集成矩阵，更新 X7 状态。
- [x] 运行 focused test、fmt、严格 clippy、serial tests 与 `just ci`。
- [ ] 提交并归档子任务，再归档父任务和记录 journal。

回滚：移除 ignored benchmark 并内联单结果 helper；该变更不涉及数据或 schema。
