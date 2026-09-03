# reset/rebuild 与定价恢复一致性

## Goal

全局 reset 清掉 `source_file`；一次 rebuild 对所选源要么全部 reset 要么全部不 reset；catalog 崩溃恢复使用 in-progress 目标，而不是旧 active meta。

## Background

- `Store::reset_usage_data`（`schema.rs:277-290`）删除 events/turns/tools/buckets/projects/cursors/sync_status/raw，不删 `source_file`。`reset_for_source_tx` 会删。09-03 测试任务把“保留 integration_install”写成现有语义；本任务改变的是 `source_file`，需更新该测试。
- `sync/engine.rs:513-518` 按源循环 `reset_for_source`。中途失败留下半重建库。
- `pricing_catalog.rs:437-445` 注释写重放 in-progress 目标，代码读 `active_pricing_catalog()`。

## Requirements

- R1. `reset_usage_data` 在同一事务删除 `source_file`（所有 host）。仍保留 `run_log` / `integration_install` / `trigger_state`。
- R2. 多源 rebuild 的 reset 在一个 `write_transaction` 中完成；任一步失败则全部回滚。随后 parse 仍可按源进行（parse 失败不要求回滚已成功 reset——若做不到，design 必须写明“reset 与 parse 的原子边界”并加测试）。默认：reset 阶段原子；parse 阶段保持现有按源失败语义。
- R3. catalog apply 崩溃恢复加载 in-progress 目标文件/身份；目标缺失或无效则失败关闭，不把旧价当成新 overlay 已生效。
- R4. 更新与 R1 冲突的现有测试（`reset_usage_data` 后 `source_file` 应为 0）。

## Acceptance Criteria

- [ ] AC1. `reset_usage_data` 后 `source_file` 行数为 0，`run_log` 仍在。
- [ ] AC2. 双源 rebuild 在第二源 reset 失败（failpoint 或注入）后，第一源用量行仍在。
- [ ] AC3. 写入 overlay 文件后、切换 meta 前崩溃：恢复要么完成 overlay，要么仍用旧 catalog，不得静默用旧价清掉 in-progress 标记并宣称成功。
- [ ] AC4. write fencing 仍包住这些事务。

## Out of scope

- 删除 `trigger_state` 表（兼容保留）。
- OpenCode cursor 原子性（上一子任务）。
