# 恢复写入 fencing 与 bootstrap 排他性

## Goal

确保 lock generation 变化后旧 worker 无法继续写入，并把 bootstrap/migration 与同步、catalog、hook 等 mutation 纳入同一排他协议。

## Confirmed Evidence

- `src/store/lock.rs:44` 的 heartbeat 遇到 `LockLost` 只记录日志，不通知业务任务停止。
- `src/commands/sync.rs:100` 在获取 worker lock 前执行 `bootstrap()`。
- 当前写事务没有携带或验证 generation；旧 holder 仍可在 lease 被 steal 后提交。
- ADR-0002 要求 `SyncRunWriter::commit_shard` 是唯一 shard 写协议入口，可作为 fencing 强制点。

## Requirements

- heartbeat 将永久失锁状态传播到持锁任务，`LockLost` 后不再刷新也不允许继续 mutation。
- 引入不可伪造的 write permit/fence token，所有写事务在提交前校验 owner + generation。
- `bootstrap()`、migration、sync、catalog apply、hook/manual repair 的写路径使用同一协调器或明确拒绝并发。
- 读命令不得为方便而隐式触发长 mutation。
- lock acquisition、heartbeat 和 transaction failure 使用稳定 typed error。

## Acceptance Criteria

- [x] two-process test：A acquire/暂停，B steal，A 恢复后下一次 heartbeat 或 write 返回 `LockLost`，A 无提交。
- [x] 旧 generation 在每个 mutation transaction boundary 都稳定失败。
- [x] `bootstrap()` 不能在 worker lock 之前运行；并发 bootstrap/sync 不产生交错迁移或写入。
- [x] sync、catalog、hook 写路径都通过统一 permit，CI 有防绕过检查。
- [x] lock 正常续租、取消和 drop 行为不回归。

## Out of Scope

- 不重构全部 Store façade，不处理与 mutation 无关的 query 性能。
