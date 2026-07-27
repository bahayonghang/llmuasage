# Bounded JSONL 读取与协作取消

## Goal

在已修复 partial-tail cursor 的基础上，完成 JSONL 单行内存上限、malformed 诊断和 blocking parser 协作取消，形成共享读取协议。

## Confirmed Evidence

- `src/parsers/file_state.rs:184` 仍使用无界 `read_line`。
- `src/parsers/claude.rs:413` 对 malformed 完整行静默跳过；其他 JSONL parser 存在同型路径。
- `src/parsers/claude.rs:191` 的 blocking parser task 未收到 cancellation token。
- 旧任务承诺的默认 4 MiB limit、诊断计数与 worker drain 语义尚未实现。
- ADR-0006 定义 durable file state/cursor，不允许诊断修复破坏幂等推进。

## Requirements

- 所有被动 JSONL parser 复用一个 `BoundedJsonlReader`/等价抽象。
- 默认最大完整行 4 MiB；超限时有界丢弃/错误，不为整行分配无界内存。
- malformed 完整行增加 source/path/offset 的有界采样与总计数，不记录原始敏感内容。
- cancellation token 进入 blocking loop，按固定行数或字节预算检查；取消后 registry 保持 cancelling 直到 worker drain。
- durable cursor 只推进到已完整消费的 record boundary；partial-tail 行为不得回归。

## Acceptance Criteria

- [x] 10 MiB 单行不会造成近似 10 MiB+ 无界分配，返回稳定 issue/counter。
- [x] malformed 完整行可在 sync summary/diagnostics 观察，日志不包含原始 prompt/content。
- [x] 取消大文件 parse 后 CPU/I/O 在秒级停止，job 在 worker 退出前不标记 cancelled。
- [x] Codex、Claude、Kimi、Pi 共享同一 contract test 套件。
- [x] partial-tail、UTF-8 边界、append-after-tail 和 cursor retry 测试继续通过。

## Out of Scope

- 不改变各来源 token accounting 规则，不把 malformed 行自动修复为事件。
