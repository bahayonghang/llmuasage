# TUI sync 后台化执行计划

- [ ] 先写纯状态机/进度文案/重复触发与取消失败测试。
- [ ] 给事件循环接入 runtime handle、JobRegistry、receiver 与 active job 状态。
- [ ] 删除 `run_sync_action` 的 runtime 构建与 `block_on`，改为 start/cancel。
- [ ] Tick 排水进度；终态更新状态、失效缓存并刷新当前面板。
- [ ] 实现退出 cancel + 500ms 有界等待，验证终端清理路径。
- [ ] 运行 focused tests、`cargo fmt --check`、严格 clippy、串行全测。

回滚：保留现有同步摘要纯文案；若 registry 接线失败，回退整个提交，不恢复 nested runtime。
