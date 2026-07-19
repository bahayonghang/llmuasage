# 实施计划

## 1. Red Tests And Baselines

- [x] 在 `tests/sync_regression.rs` 增加多项目 Claude 夹具：一个项目含跨文件 streaming/sidechain 候选，另一个项目保持不变；追加后断言当前实现错误扫描其他项目（先红）。
- [x] 强化 Codex 增量测试：两文件中只追加一个，断言 `changed_files=1`、另一文件 skipped、`bytes_scanned` 只包含追加范围（当前应绿，作为保护）。
- [x] 强化 OpenCode 增长测试：原 DB 追加一条 message 后断言只观察新增行；当前 identity 实现应红。
- [x] 增加 OpenCode part 热跑测试：首次导入、无变化、追加一个 part、替换 DB 四阶段；当前全表扫描/无 cursor 实现应红。
- [x] 增加 migration query-plan/index 断言与 shard 行为事实预去重单测。

Focused commands:

```powershell
cargo test --test sync_regression claude_changed_project_does_not_replay_other_projects -- --exact --test-threads=1
cargo test --test sync_regression codex_append_scans_only_changed_file -- --exact --test-threads=1
cargo test --test sync_regression opencode_growth_keeps_incremental_cursors -- --exact --test-threads=1
cargo test --test sync_regression opencode_part_scan_uses_persisted_high_water -- --exact --test-threads=1
```

## 2. Migration And Cursor Contract

- [x] 在 `src/store/migrations.rs` 追加真实 v15 migration，并覆盖 fresh/upgrade/idempotent 形态。
- [x] 扩展 `OpencodeCursor`、`CursorStore::load_opencode_cursor` 与 `save_opencode_cursor`，默认兼容旧行。
- [x] 添加两个行为 reset 索引存在性和查询计划测试。

Rollback point: v15 尚未被真实库验证前，不运行用户数据库 bootstrap；全部使用 TempDir 或在线备份。

## 3. Claude Work-Set Repair

- [x] 让 source listing 暴露枚举 root，并实现可测试的顶层 project key。
- [x] 重写 Claude plan 构造：先判定 trigger，再只为选中项目构造全项目 replay plan；不包含 missing cursor path。
- [x] 保留有界 `spawn_blocking` 与单 writer commit，修正 changed/skipped/progress 统计；同一有界解析批次合并为一次事务。
- [x] 验证 streaming/sidechain 去重、首次导入、热跑、append、truncate/replace、missing/deleted 和 cancellation。

## 4. Writer Hot Path

- [x] 在事务前按稳定 key 保序去重 shard 内 turns/tool calls。
- [x] 依赖 v15 path 索引与临时 path 表，把行为 reset 收敛为两条集合 DELETE。
- [x] 将 touched bucket pricing refresh 从逐 bucket source-range 重扫改为临时 bucket 表 + 一次 source-range 扫描，并用共享 bucket mixed rate 回归锁定语义。
- [x] 重跑 Claude 红灯夹具并记录 parse/write 分解；按证据合并 Claude 有界解析批次的 writer 事务。

## 5. OpenCode Incremental Scan

- [x] 用持久化 `(last_time_created, last_processed_ids)` 锚点存在性区分正常增长与数据库替换；拒绝受 NTFS tunneling 影响的 Windows creation time 方案。
- [x] 数据库替换时同时重置 message 与 part cursors。
- [x] 实现封闭上界 + rowid 分页 part 扫描、页间取消、稳定 fallback key 和完成后 cursor 推进。
- [x] 将 tool payload 纳入 bytes/work stats，保持缺表降级。
- [x] 用真实 90 MB OpenCode DB 的只读查询计划复核 `rowid` range search，目标库使用在线备份。

## 6. Cross-Source Verification

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test --test sync_regression -- --test-threads=1`
- [x] `cargo test --test source_file_state -- --test-threads=1`
- [x] `cargo test --test token_accounting_parity -- --test-threads=1`
- [ ] `cargo test -- --test-threads=1`
- [x] `CI=1 cargo test -- --test-threads=1`（308 unit + 全部 integration/doc tests；仓库自带 CI 性能阈值）
- [x] `npm --prefix docs run docs:build`
- [x] `git diff --check`
- [x] 用同一在线备份/夹具记录 Claude、Codex、OpenCode 修复后 `bytes_scanned`、parse/write 和 wall time；不得把缺失测量写成 pass。

## 7. Documentation And Review

- [x] 更新 `.trellis/spec/llmusage/backend/source-sync-contracts.md`：项目级 replay、cursor anchor、part cursor、集合 reset 和 stats 语义。
- [x] 按实际 schema 决策更新 ADR 0004 的 v15 记录；`SyncShard` 协议未变，不新建 ADR。
- [x] 检查最终 diff 只包含本任务和用户已有 README/Trellis 改动，绝不覆盖或顺带提交用户 WIP。

## 8. Performance Evidence

- 修复前 Claude：1 个变化文件触发全源 899,626,059 bytes；总 106.5s，parse 4.1s，write 102.1s。
- 仅项目级 replay + v15 索引后：363/704 文件、588,938,537 bytes；总 82.802s，parse 1.269s，write 81.305s，证明 writer 仍有独立放大。
- writer 查询复现：124 文件项目触及 169 buckets；旧逐 bucket refresh 16.142s，同一数据一次 source-range scan + 临时 bucket PK join 0.102s。
- 最终 release 在线备份：380/704 文件、586,432,745 bytes；总 8.863s，parse 2.554s，write 6.028s。相对上述同量级 writer 基线约快 13.5 倍；热跑 344ms、0 bytes。
- Codex 最终增量：4/1796 文件、13,784,961 bytes、1.484s；紧接着的活动会话追加仅扫描 1,075 bytes。
- OpenCode v15 part backfill：37,243,250 bytes、1.641s；热跑 170ms、0 bytes。

## 9. Debug Retrospective

### Root Cause Category

- **E / Implicit assumption**：项目级 replay 修复假定缩小扫描工作集即可解决总耗时，但 writer 的 bucket pricing refresh 仍为 `touched_buckets × source_events`。
- **D / Test coverage gap**：原测试覆盖结果正确性，没有限制扫描字节、part 行范围、SQL query plan 或 writer parse/write 分解。

### Prevention Mechanisms

- **DONE / Test**：三来源回归断言实际工作集合；v15 断言行为 reset 索引；shared-bucket 测试断言一次 source-range scan 与 mixed pricing 恢复。
- **DONE / Spec**：source-sync contract 禁止用 DB 内容 metadata 充当代际、禁止逐 bucket source-range 重扫，并固定 Claude 项目边界与 OpenCode rowid cursor。
- **DONE / Process**：性能修复必须同时报告 bytes/rows、parse/write 与 wall time；若 writer 仍占主导，不得以 parser 改善宣称完成。

### Residual Gate

- `[x]` 子任务 `.trellis/tasks/07-19-home-overview-query-performance` 已完成：`home_overview` 80ms 门未放宽，debug/release 各连续 3 次通过；summary/by-platform/series 改为共享一次 filtered `usage_event` row stream，diagnostics 仅为缺失 source 查询 bucket。
- `[x]` 子任务保留跨日 session、fallback identity、四平台默认键、filter/timezone、bootstrap/archive/last_updated 逐字段回归，并在 132,279-event online backup 上完成只读阶段 profiling。
- `[x]` 本父任务标准门禁已在子任务合并工作树上重新运行：fmt、严格 Clippy、focused query tests、未设置 `CI=1` 的完整串行测试、docs build、git diff check 全部通过。
