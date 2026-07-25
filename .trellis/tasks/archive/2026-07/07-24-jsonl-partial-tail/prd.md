# JSONL 尾部半行游标漏记修复（DATA-001，含 DATA-006/REL-005）

## Goal

消除 JSONL parser 在并发追加场景下把未写完的 EOF 尾行计入 durable cursor 导致的**永久漏记**，并一并解决 parser 的错误可见性、单行大小上限与协作取消问题（同属 parser 读取层，审计建议统一为 `BoundedJsonlReader`）。

## 覆盖发现（已核实）

- **DATA-001（P1）**：`src/parsers/claude.rs:405-418`（codex.rs:380-399,505-512、kimi_code.rs:329-397、pi.rs:329-407 同型）：`read_line` 后先 `offset += bytes_read` 再 parse；EOF 半行 parse 失败被 `continue`，返回的 `end_offset` 已越过半行。producer 补完该行后，下次 sync 从半行中部继续读，事件永久丢失。
- **DATA-006（P2）**：完整但 malformed 的 JSON 行被静默 `continue`，无 parse-error 计数/path/offset 诊断；`read_line` 对单行长度无上限，超大行可导致巨量内存分配。
- **REL-005（P2）**：`src/parsers/claude.rs:182-230`（其他 parser 同型）cancel 只在 batch/task await 边界检查；已 `spawn_blocking` 的 parser 闭包不读取 cancellation token，取消后 CPU/IO 仍持续。

## Requirements

1. durable offset 只推进到最后一个以 `\n` 结束的完整行；`!line.ends_with('\n')` 且 EOF 时，`end_offset = line_start`，下次从完整行起点重读。
2. 半行不交给 durable parser（或允许 parse 但不推进 durable cursor）。可选"文件 N 秒未变化后接受 EOF 无换行完整 JSON"的稳定期策略。
3. 四个 parser 共享同一读取抽象（`BoundedJsonlReader` 方向），避免四份逻辑继续漂移；抽象包含：complete-record cursor、max line bytes（默认 4 MiB 可配置）、cancellation 检查（每 N 行/字节）、parse issue 计数与采样诊断（path/offset）。
4. malformed 完整行的跳过必须产生可见计数（sync summary / diagnostics），不得静默。

## Acceptance Criteria

- [ ] contract test：写半行 → sync → 追加余下半行 → sync，最终恰好 1 个 event，无重复无遗漏（在旧实现上稳定失败）。
- [ ] 半行跨 UTF-8 多字节边界、半行接近 max line limit 两个边界场景通过。
- [ ] sync 期间 producer 连续追加多行 → 无漏记。
- [ ] cursor commit failpoint 后重试仍幂等。
- [ ] 单行 10 MiB → 有界错误与诊断计数，不 OOM。
- [ ] 取消 blocking parse：状态保持 cancelling 直到 worker drain，CPU/IO 在秒级停止。
- [ ] Codex、Claude、Kimi、Pi 四 parser 复用同一 contract test 套件全部通过。

## Notes

- 审计报告 §1.2 DATA-001 深挖含完整时间线与修复协议；§6.1 数据完整性测试矩阵。
- 复杂任务：启动前需补 design.md（reader 抽象与 cursor 协议）+ implement.md。
