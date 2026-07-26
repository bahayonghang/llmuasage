# Design: Write Fencing Closure

## Core Mechanism

- `WorkerLock` 持有 owner/generation，并生成只能由 store 内部构造的 `WritePermit`。
- heartbeat 共享一个 lost-state/cancellation signal；`LockLost` 只设置一次并唤醒调用方。
- `SyncRunWriter` 和其他 mutation repository 在事务开始及 commit 前校验 permit；校验 SQL 与实际 mutation 位于同一 SQLite transaction。
- schema/bootstrap acquisition 在命令层之前进入 application-level `OperationCoordinator`，避免先写后锁。

## Boundaries

- parser 只构造 `SyncShard`，不感知 generation。
- CLI/Web/hook adapters 只请求 operation，不直接 acquire/refresh lock。
- read-only operation 不要求 permit，也不得调用会写 schema/catalog 的隐式 bootstrap。

## Failure Semantics

- heartbeat 暂时性 I/O 错误可按现有策略报告；明确 `LockLost` 是终止性错误。
- 已完成的原子 shard 保留；失锁后的下一 shard 不开始，正在提交的事务由同事务 fence check 决定。

## Tests

- deterministic lock-steal test 使用两个独立 Store/connection，不依赖 sleep 竞态。
- transaction-level tests 覆盖 sync shard、catalog 和 bootstrap/migration。

