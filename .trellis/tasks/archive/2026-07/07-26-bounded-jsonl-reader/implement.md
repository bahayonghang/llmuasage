# Implementation Plan: Bounded JSONL Reader

- [x] 建立跨四 parser 的 contract test harness。
- [x] 加入 oversized、malformed 和 cancellation 回归测试。
- [x] 实现 bounded record reader 与 durable offset 协议。
- [x] 将 cancellation token 传入 blocking parse/discard loops，并修正 terminal event 时序。
- [x] 将四个 parser 迁移到共享 reader，删除重复无界 `read_line` 路径。
- [x] 接通 sync summary/diagnostics issue counter，并验证隐私边界。
- [x] 运行 parser contracts、sync regression、完整 Rust tests 与 `just ci`。
  - parser contracts 57/57、cancellation drain 1/1、parse issue/schema v17 3/3 通过。
  - `python scripts/ci-rust.py` 通过；`just ci` 最终复跑通过 529 个 Rust 单测、全部 integration/doc tests、dashboard JS 检查与 VitePress 构建。

## Rollback

- 先迁移一个 parser 验证 reader，再逐源迁移；任何回退不得恢复 partial-tail cursor 缺陷。
