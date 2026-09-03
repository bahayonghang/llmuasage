# OpenCode/ZCode 同步健壮性

## Goal

OpenCode 源库只读打开且有 busy timeout；工具 JSON 解析失败进入 parse_issues；OpenCode/ZCode 的 cursor 与最后一次 shard 提交在同一写事务里。

## Background

- `opencode.rs:121` `Connection::open` 读写、默认 busy=0。Antigravity/ZCode 已是 `SQLITE_OPEN_READ_ONLY`。
- `opencode.rs:266-268` 工具 part JSON `from_str` 失败 `continue`，不记 parse_issues。
- OpenCode 每页 `commit_shard`，cursor 稍后 `save_opencode_cursor`（`:302-307`）。ZCode 每页后另开 `write_transaction` 存 cursor。崩溃后靠 `INSERT OR IGNORE` 重放，不是原子提交。

## Requirements

- R1. OpenCode 用 `SQLITE_OPEN_READ_ONLY` + 非零 busy timeout（与其他源 DB 读者同级，建议 ≥1s）。
- R2. 源库 `SQLITE_BUSY` / 打开失败不得当成空库成功并推进高水位。
- R3. 工具 part JSON 失败计入 `parse_issues`（或等价诊断），不静默丢。
- R4. OpenCode/ZCode cursor 写入与对应 shard（至少最后一页，或每页）同一 `commit_shard` 事务 / 同一 writer 事务。若协议要把 cursor 放进 `SyncShard`，更新 ADR 0002 注释。
- R5. 崩溃重放仍幂等，不重复计数。

## Acceptance Criteria

- [ ] AC1. OpenCode 打开 flags 含 READ_ONLY；测试断言 busy timeout > 0。
- [ ] AC2. 损坏的 part JSON 使 `parse_issues.malformed_lines`（或同类计数）增加。
- [ ] AC3. 单测或 failpoint：shard 提交成功而进程在 cursor 独立事务前退出的窗口消失（cursor 与事件同进同退）。
- [ ] AC4. 不改 Antigravity 打开失败语义（已由 P0 子任务覆盖）。

## Out of scope

- 全局 rebuild 事务（`09-03-store-reset-rebuild-recovery`）。
- 进度事件里的绝对路径脱敏（可在本任务顺手，非 AC）。
