# worker lease fencing 与统一写协调（CONC-001/ARCH-001）

## Goal

关闭跨进程双写窗口：为 worker lease 引入 fencing generation，并把 bootstrap/pricing/catalog/reset/migration 等所有 mutation 收敛到同一写协调器（OperationCoordinator），使"全局写锁"真正覆盖全部写入口。

## 覆盖发现（已核实）

- **CONC-001（P1）**：`src/store/lock.rs:160-266`：lease 过期可被新 owner 接管（steal）；`refresh_worker_lock`（246-257 行）与 `release_worker_lock` 忽略 affected row count；heartbeat 失败只 warn。进程 A 挂起超 lease 后恢复，heartbeat UPDATE 影响 0 行但返回 Ok，A 不知锁已丢，与新 owner B 同时写，cursor/reset/bucket 交错。
- **ARCH-001（P1）**：`src/store/schema.rs:63-108`：`bootstrap()` 内调用 `upgrade_embedded_pricing_if_needed`（101 行）执行 pricing 升级；`src/commands/sync.rs:314-321` sync 在拿 worker lock **前**先 bootstrap；serve（`src/commands/serve.rs:49-50`）与 catalog 命令也直接 mutation；`src/store/sync_writer.rs:76-107` writer 启动时 snapshot catalog。repricing/migration/catalog apply 可与 sync 并发，同一 DB 出现不同价格版本写入。

## Requirements

1. worker_lock 表增加 `generation INTEGER NOT NULL`；acquire/steal 在 `BEGIN IMMEDIATE` 中 generation+1；`WorkerLock` 持有 `(owner_id, generation)`。
2. heartbeat/refresh 使用 `WHERE owner_id=? AND generation=?` 并要求 affected rows == 1；0 行即触发 cancellation token，业务任务感知 `LockLost` 并停止提交。
3. 每个 write transaction（commit_shard、pricing activation、reset、migration）在事务开始时校验当前 generation，旧 generation 写入必须失败。
4. 新建 OperationCoordinator：schema/pricing/reset/sync 等全部 mutation 走同一 fenced write service；read command 的 bootstrap 不得触发长业务迁移（pricing 升级改为显式 maintenance operation）。
5. hook/manual/catalog/serve repair 的 operation 优先级与冲突策略需在 design.md 明确。

## Acceptance Criteria

- [ ] two-process 测试：A acquire → 挂起 → B steal → A 恢复，A 下一次 heartbeat/write 得到 `LockLost`，无交错提交（在旧实现上稳定失败）。
- [ ] 旧 generation 的每个 write transaction 都失败。
- [ ] sync 与 catalog apply 并发时只有一个 fenced writer 能 commit。
- [ ] sync 与 migration/bootstrap 明确串行或拒绝，不隐式并发。
- [ ] read-only 命令（report/serve 只读路径）不再隐式执行 pricing 长迁移。
- [ ] chaos test：suspend/resume + 竞争进程场景通过。

## Notes

- 本任务是 07-24-pricing-atomicity 的**前置**（审计 §5：先 F 后 G/I）。
- 审计报告 §1.2 CONC-001 深挖与 §3.2.1；schema migration 需新增版本。
- 复杂任务：启动前需补 design.md + implement.md。
