# Implementation and validation

1. [x] 固定两个上游检查点，原生 descriptor/13样本/独立 oracle 关闭语义问题；用户授权 source split 与实施。
2. [x] CLI shared decoder、steps/retry/typed time、多身份、WAL 与产品组重放实施；marker3 原子修复保护 hook history。
3. [x] IDE native source 与所有消费者接入，README/docs/ADR0017/spec 同步。
4. [x] 跨面各 gate 与独立 review 完成，最终原生 busy/cancel/单输出通道测试通过；Windows CLI110DB/IDE530DB 全量独立对账与幂等通过，逐项证据见 research/implementation-validation.md。
5. [x] 写入 Trellis session 和不可变 Basic Memory checkpoint；本轮不 commit/push/archive。

对父/两 child 运行 task.py validate，结构检查与产品验收分别记录。大 source-sync spec 按 index 直接分段读，不依赖截断注入。任务保留 in_progress 直到另行授权 commit/archive。
