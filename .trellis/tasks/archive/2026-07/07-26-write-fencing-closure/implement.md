# Implementation Plan: Write Fencing Closure

- [x] 为 heartbeat lost-state 增加旧实现失败的测试。
- [x] 定义 `WritePermit` 与 lost signal，禁止外部裸构造。
- [x] 将 generation 校验下沉到 mutation transaction 边界。
- [x] 将 sync/bootstrap/catalog/hook mutation 接到统一 coordinator。
- [x] 删除或封闭绕过 permit 的 public mutation 入口。
- [x] 增加 two-process steal、并发 bootstrap 和正常取消回归测试。
- [x] 运行 store/lock/sync focused tests、完整 Rust tests 与 `just ci`。

## Risk And Rollback

- 风险集中在 schema 初始化和测试 helper；先迁移生产入口，再收紧可见性。
- 保持每类 mutation 独立测试，若某路径迁移失败可回退该提交而不回退 generation schema。
