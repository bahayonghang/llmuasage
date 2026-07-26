# Implementation Plan: Windows Atomic Integration Replace

- [x] 在 Windows target dependencies 中声明最小 feature 集的 `windows-sys` 直接依赖。
- [x] 抽取可注入 failpoint 的 atomic replace backend。
- [x] 增加覆盖旧 Windows 删除窗口的 existing/missing target 回归测试。
- [x] 实现 Windows `ReplaceFileW` 和目标不存在路径；保留 Unix 行为并补 durability。
- [x] 为 action 记录增加 pending intent 与补偿式恢复协议。
- [x] 将所有 integration 写入点迁移到统一 writer。
- [x] 运行 integration focused tests、Windows tests、完整 Rust tests 和 `just ci`。

## Rollback Point

- OS replace backend 与 action protocol 分成两个独立提交；任一失败均保留现有备份可恢复性。
