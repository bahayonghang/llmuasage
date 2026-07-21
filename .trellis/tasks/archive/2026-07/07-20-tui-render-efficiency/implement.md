# TUI 渲染效率执行计划

- [ ] 先写 redraw 决策表、空闲 10s 零 draw、动画 tick draw 与 Tick 折叠测试。
- [ ] 接入 dirty 生命周期和后台结果排水顺序。
- [ ] 实现每帧 Theme 快照并迁移 nav/footer/dialog/面板 API。
- [ ] 以 generation 为失效键实现格式化 memo 与可见行切片。
- [ ] 扩展 Blocks、Models、Cost 的 TestBackend buffer 等值测试。
- [ ] 记录 draw 计数/锁次数/分配或 wall-time 证据。
- [ ] 运行 focused tests、fmt、严格 clippy、串行全测。

回滚：dirty、theme snapshot、row memo 各自形成验证点；任何 buffer 差异先修复，不更新 golden 掩盖回归。
