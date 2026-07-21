# sync 冷跑全量导入写入吞吐优化（backlog）

父任务：`.trellis/tasks/07-20-sync-progress-and-perf`。由 `07-20-sync-full-profiling` 的测量立项（证据：`.trellis/tasks/07-20-sync-full-profiling/research/profiling.md` §5 P1）。

## Goal

提升全量导入（冷跑）场景的 writer 提交吞吐。基线：release 构建、104,110  Codex 事件，write 25.3s（约 4.1k events/s），占冷跑总耗时 78%。

## Confirmed Facts

- 冷跑分解：codex parse 2.2s / write 25.3s；claude 1.4s / 2.8s；opencode 243ms / 194ms。写入是绝对主导。
- writer 协议：单 `SyncRunWriter`，每 `commit_shard` 一个 Immediate 事务：reset → 批量 event 写入（`EVENT_WRITE_BATCH_SIZE`）→ cursor → source_file 标记 → 可选 raw archive（src/store/sync_writer.rs:489-560）。
- 增量（热跑）路径已无问题：hot median 493ms。

## Requirements

- R1 先测量后修复：定位 write 25.3s 的构成（INSERT OR IGNORE 冲突检查、索引维护、事务/fsync 频率、raw archive、行为事实写入），给出各项占比。
- R2 候选方向（需测量验证）：批量大小调优、事务粒度（每 shard 一事务 → 有界多 shard 一事务）、索引/pragma 评估、shard 内预去重命中率。
- R3 契约红线：不违反 `.trellis/spec/llmusage/backend/source-sync-contracts.md` §3 —— 单写者、reset→event→cursor 原子提交顺序、取消边界语义；不要求用户 rebuild。
- R4 验收对照：同 profiling 协议（快照恢复、3 次中位数），冷跑 write 耗时显著下降且全量/增量语义回归全绿。

## Acceptance Criteria

- [ ] A1：write 路径耗时构成测量报告（各构成占比）。
- [ ] A2：修复后同协议对照数据，write 耗时下降有证据。
- [ ] A3：`cargo test -- --test-threads=1`、fmt、严格 clippy 通过；取消/中断语义测试不受影响。

## Out Of Scope

- 增量路径（热跑）优化；parser 解析侧优化；schema 语义变更。
