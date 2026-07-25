# pricing 重算原子性与崩溃恢复（DATA-002）

## Goal

消除 pricing recompute 中途失败产生的长期 mixed pricing state：事件成本、bucket 汇总与 catalog metadata 要么全部对应同一价格版本，要么可自动恢复到单一版本。

## 覆盖发现（已核实）

- **DATA-002（P1）**：`src/store/mod.rs:346-519`：event cost 按 5,000 行分页、每页独立事务提交（352、380-421 行）；bucket rollup 全程放在进程内 `HashMap`（378 行）；全部完成后才在另一事务 reconcile bucket 并切换 metadata。`src/store/pricing_catalog.rs:201-267,398-415`：custom overlay/snapshot 无 durable in-progress journal。中途 crash 后：部分 event 新价、部分旧价、bucket/metadata 仍旧，状态可长期存在且无法被自动识别。

## Requirements

优先方案 A（versioned cost）或短期方案 B（durable journal），二选一并在 design.md 论证：

- **方案 A**：`usage_event_cost(event_key, pricing_version, ...)` / `usage_bucket_cost(bucket_key, pricing_version, ...)`；后台构建新 version 不影响 active read；全部完成并校验后在一个小事务中切 `active_pricing_version`；旧 version 延迟 GC；sync writer 按 active version 写新事件。
- **方案 B**：`pricing_operation` journal（operation_id/from_version/to_version/phase/last_event_key/error）；每页 commit 同步更新 `last_event_key`；bootstrap 发现非 done operation 必须 resume 或显式 rollback，禁止正常读写假装一致。

约束：

1. 依赖 07-24-write-fencing-coordinator 的 OperationCoordinator；pricing mutation 必须持有 fenced WritePermit。
2. `doctor` 需能校验 metadata version、event version、bucket version 三方 invariant。

## Acceptance Criteria

- [ ] 在第 1、2、最后一页之后注入 crash（failpoint），重启能自动恢复到单一 version（在旧实现上稳定失败）。
- [ ] 在 bucket reconcile 前 crash，读 API 不返回 mixed data。
- [ ] sync 与 catalog apply 并发，只有一个 fenced writer 能 commit。
- [ ] `SUM(event cost)` 与对应 bucket cost 在 tolerance 内一致。
- [ ] `doctor` 三方 invariant 检查全绿并进入 CI fixture。

## Notes

- **前置依赖**：07-24-write-fencing-coordinator（审计 §5：pricing 新实现不得在旧写入口可绕过时落地）。
- 审计报告 §1.2 "ARCH-001 + DATA-002" 深挖含状态机表与两方案细节；§3.2.2。
- 复杂任务：启动前需补 design.md + implement.md。
