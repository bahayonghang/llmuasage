# 多来源同步扫描性能设计

## 1. Evidence And Root Causes

### Claude

`src/parsers/claude.rs:128-159` 先正确计算变化文件数，但只要结果非零，就把全部文件以 `existing: None` 放入唯一 `ClaudeShardPlan`，并 reset 全部 cursor path hash。这个行为由 `0848fe8` 为全局 message/sidechain 去重引入，使 `parallelism` 因只有一个 plan 而失效，也让统计中的 skipped 与实际重放工作不一致。

真实基线中解析 899.6 MB 只占约 4.1 秒，102.1 秒主要发生在 shard commit。`usage_event` 已有 `(source, source_path_hash)` 索引，但 `usage_turn` 与 `usage_tool_call` 没有对应索引；writer 对每个 reset path 分别删除行为事实时，只能重复扫描该 source 的行为行。Claude shard 还把大量重复 streaming turn/tool key 交给 SQLite 做 `INSERT OR IGNORE`。

### Codex

`src/parsers/codex.rs:119-149` 已按 cursor 选出变化文件并按日期目录分片。真实基线只扫描 3 个变化文件/7.4 MB，没有证据支持重写该游标协议。Codex 只接受共享行为事实预去重、测试强化和不改变语义的共享 planner 整理。

### OpenCode

`src/common/util.rs:149-156` 的 `file_identity` 包含长度、mtime 和头签名；SQLite 正常增长就改变 identity，`src/parsers/opencode.rs:100-109` 因而清空消息高水位。`src/parsers/opencode.rs:482-517` 又在每轮同步全扫 `part`，没有持久化工具事实水位。

真实查询计划确认 message 时间水位缺少理想索引但当前成本较小；`part` 查询明确 `SCAN part` 并使用临时 B-tree。llmusage 不得修改 OpenCode 自有数据库，因此增量键采用 SQLite 隐式 `rowid`，并以一次扫描开始时的 `MAX(rowid)` 作为封闭上界。

## 2. Architecture

### 2.1 Claude Project-Scoped Replay

`SourceFileListing` 保留枚举 root。Claude 以 `root` 下第一个路径组件作为 project key，先对所有文件执行轻量 metadata/cursor 判定，再选择“至少有一个变化文件”的项目。

每个选中项目生成一个 plan：

- 包含该项目当前存在的全部 JSONL，保证 streaming/sidechain 跨文件去重完整；
- 只 reset 该项目当前存在且已有 cursor 的 path hash，不删除 missing 文件保留的历史事实；
- 候选以全文件 reparse 模式进入 plan，因为项目级 dedupe 需要看到项目内完整候选集；
- 不同项目形成独立 plan，由现有有界 `spawn_blocking` 批次并行解析；同一有界批次的项目输出合并为一个 `SyncShard` 提交，项目内 logical dedupe 不跨边界，但共享一次 writer 事务与 checkpoint。

Claude 的顶层项目是去重隔离边界：真实 73,607 个候选中没有 message ID 跨该边界；稳定 logical event key 仍由 SQLite 主键提供最终全局幂等保护。

统计按实际重放文件计算：选中项目内文件计入 `changed_files`，未选中项目文件计入 `skipped_files`。内部可另保留 trigger count 用于 tracing，但不扩展公开 wire contract。

### 2.2 Indexed Behavior Reset And Pre-Dedupe

新增 migration v15：

- `source_cursor.last_part_rowid INTEGER NOT NULL DEFAULT 0`；
- `idx_usage_turn_source_path_hash ON usage_turn(source, source_path_hash)`；
- `idx_usage_tool_call_source_path_hash ON usage_tool_call(source, source_path_hash)`。

`SyncRunWriter` 在开启事务前按 `turn_key` / `tool_call_key` 对 shard 内行为事实保序去重。数据库仍保留 `INSERT OR IGNORE` 作为跨 shard/跨运行的最终幂等边界。reset、events、cursor、source_file、behavior facts 仍在同一 `commit_shard` 事务内，符合 ADR 0002。

修复后测量又确认两处集合放大：behavior facts 对每个 path 分别执行两次 DELETE；bucket pricing refresh 对每个 touched bucket 都用仅约束 `source` 的查询重扫整段 `usage_event`。实际实现分别使用临时 path 主键表配合两条集合 DELETE，以及临时 bucket 主键表配合一次 `idx_usage_event_source_path_hash` source-range 扫描。数值 bucket rollback 仍按 path 索引执行，未改变增量聚合语义。

### 2.3 OpenCode Cursor Anchor Replacement Detection

不再用会随 SQLite 正常增长变化的长度、mtime 或头签名判断数据库代际。同步开始时验证持久化消息高水位锚点：

- `last_time_created == 0` 或 `last_processed_ids` 为空时视为尚无可验证锚点，不重置；
- 否则逐个确认 `(time_created, id)` 仍存在于当前 `message` 表；
- 全部存在表示原库正常增长，保留 message 与 part 水位；任一缺失表示替换/截断，三项水位一起归零。

Windows creation time 没有采用：NTFS tunneling 可让同路径替换继承创建时间，不能可靠证明数据库代际；稳定 file-index API 在当前 Rust 跨平台边界也不成熟。锚点存在性直接验证 parser 真正依赖的续跑事实，且不要求写 OpenCode 自有数据库。

### 2.4 Incremental OpenCode Part Scan

扫描开始先读取 `MAX(rowid)` 为 `upper_rowid`。随后分页查询：

```sql
SELECT rowid, time_created, data
FROM part
WHERE rowid > ?1 AND rowid <= ?2
  AND data LIKE '%"type":"tool"%'
ORDER BY rowid
LIMIT ?3
```

`rowid` 范围走 SQLite 内建 B-tree；`LIKE` 只在新增封闭区间过滤。每页解析和 commit，页间检查 cancellation。完成全部页面后才把 cursor 推进到 `upper_rowid`；中途取消会在下次重复最后范围，但 destination key 与 `INSERT OR IGNORE` 保证幂等。

part 表缺失时返回“不可用”而不是失败，cursor 保持不前进。fallback tool key 使用稳定 rowid，不再依赖每轮从 0 开始的枚举 index。

`bytes_scanned` 累加 message 与实际返回的 tool payload；`changed_files` 在任一 message/tool 范围有工作时为 1，无工作时 `skipped_files=1`。

## 3. Compatibility And Migration

- 不改变 CLI 参数、`SyncEvent` variant 或 JSON 字段形状。
- v15 只向 llmusage 自有库加一列和两个索引，不修改 OpenCode DB。
- 旧库的 `last_part_rowid=0` 会执行一次幂等 part backfill；已有 `usage_tool_call` 不重复计数。
- OpenCode 既有消息锚点若仍存在会直接续跑；v15 新增的 part 水位默认为 0，因此只对工具事实做一次幂等 backfill，不要求用户 rebuild。
- Claude missing/deleted 状态仍按 ADR 0006 保留，项目重放不会 reset 当前 inventory 之外的 cursor path。

## 4. Failure And Cancellation Semantics

- Claude parser task 出错或取消时当前有界解析批次不提交；此前已完成批次保持已提交，writer 原子性不变。
- OpenCode part 页 commit 失败时 cursor 不推进；重试安全。
- cancellation 只在项目/分页边界生效，不引入半个 shard。
- migration v15 在单独 `BEGIN IMMEDIATE` 中建列/索引，失败按 ADR 0004 整步回滚并保留旧 schema version。

## 5. Rollback

- 代码回滚不会删除 v15 新列/索引；旧二进制会忽略附加列和索引，schema downgrade 本就不受支持。
- 若 anchor 检测与未来 OpenCode schema 不兼容，回滚 anchor 检查与 part cursor 代码即可，保留无害 schema；不得退回长度/mtime/creation-time 猜测。
- 若 Claude project 边界测试发现跨项目语义，停止实现并回到规划，不以性能理由弱化去重正确性。
