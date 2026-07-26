# Implementation Plan: Runtime Log Bounds

- [x] 增加单进程持续写入和 dropped queue 旧实现失败测试。
- [x] 引入 size/daily-aware rotation 与序列化 retention maintenance。
- [x] 暴露 dropped counter 到 runtime diagnostics。
- [x] 更新 tail reader 跨分片反向读取。
- [x] 增加 occupied-file 删除失败/重试模拟、Windows 实际占用测试和总量预算断言。
- [x] 运行 logging/diagnostics focused tests、`cargo fmt --check`、严格 Clippy 与相关集成测试。
- [x] 由主会话运行 dashboard JS tests、完整 Rust tests 与 `just ci`。
