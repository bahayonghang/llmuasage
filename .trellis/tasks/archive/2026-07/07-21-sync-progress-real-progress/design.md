# 技术设计

> 状态：规划已批准，任务已进入实施与验证阶段。

## 1. Problem Restatement

用户需要看到文件解析已经完成了多少，而当前事件只在 shard/batch commit 后出现，并且进度条总量与实际 replay 工作量不是同一集合。

基本约束：

- 解析发生在 `spawn_blocking` worker，渲染和 mpsc 发送不能进入文件热路径。
- Claude project 是 replay/dedupe 边界；writer batch 是原子提交与 fsync 优化边界。
- 进度是观察面，不能改变 parser、cursor、commit、失败或取消结果。

## 2. Recommended Progress Contract

### 2.1 Work Set

- Claude 完成 inventory/cursor 判定并构造 project plans 后，计算 `planned_files = plans.iter().map(|plan| plan.files.len()).sum()`。
- Codex 完成 cursor 判定并构造 shard plans 后，使用各 plan 文件数之和；该值应等于既有 `changed_files`。
- 此时 emit `SourceStarted { files_total: planned_files }`。`files_total=0` 表示本轮无文件需要 replay，随后仍由 `SourceFinished.stats` 报告 inventory 与 skipped 总量。

这会修正 `SourceStarted.files_total` 对 Claude/Codex 的语义，但不改变 enum/serde 形状。OpenCode 保持现状。

### 2.2 Worker Counter

新增 parser 内部共享 helper，职责保持最小：

- 拥有 `Arc<AtomicU64>` completed counter；worker clone 仅暴露 `advance_file()`。
- async waiter 在等待既有 `JoinHandle` 时使用固定 interval 采样，`Ordering::Relaxed` 读取。
- 仅当 completed 大于 last emitted 时回调；不在 helper 内构造 `SyncEvent`，以便 Claude/Codex 各自提供 source 与当前 committed `records_imported`。
- worker 只在一个文件完整解析并写入 shard output 后 increment；文件解析返回错误时不虚报完成。

采样周期为 200ms（最大 5Hz）。indicatif 仍可按现有 10Hz draw target 绘制，但事件生产端无需以同等频率制造 LineRenderer/NDJSON 行。

### 2.3 Await And Commit Flow

对一个有界 batch：

1. 与当前相同，先启动全部 `spawn_blocking` tasks。
2. async 侧等待每个 handle 时，同时等待 progress interval；tick 只采样共享 counter 并尝试发送快照。
3. task 完成后按现有路径聚合 shard output。
4. 进入 commit 前强制 emit 当前 completed counter。最终 batch 在这里先把 TTY 推到 `N/N`，从而在同步写入期间显示“重放完成，正在提交...”。
5. Claude 在 batch 全部成功后合并一次 `commit_shard`；Codex 继续逐 shard commit。
6. commit 后再 emit 一次边界快照，即使 counter 与上次相同也更新 `records_imported`；source 成功结束前保证最后一个 Progress 满足 `files_scanned == planned_files`。

不在 `tokio::select!` 中增加新的 cancel 分支，避免 drop `JoinHandle` 后 detached blocking worker 改变现有取消语义。既有 batch/file 边界检查保持不变。

## 3. Event And Consumer Behavior

- TTY BarRenderer：length 与 position 都是 planned replay files；`complete_active` 保留为失败防御/终端清理兜底。
- LineRenderer 与 `sync --json-events`：接收相同的节流 Progress 快照；不增加输出通道。
- `records_imported` 只在 commit 后增长。解析完成但提交未完成时，文件进度可以先到 planned total，这是“解析进度与提交进度解耦”的真实状态。
- TTY BarRenderer 在 determinate bar 的 `position == length` 时显示“重放完成，正在提交...”；`SourceFinished` 仍负责关闭 bar 并输出永久统计行。该 message 是 renderer 对既有字段的展示推导，不进入 SyncEvent wire，也不要求 LineRenderer/TUI 伪造 commit 状态。
- `current_file` 保持 `None`；跨多个并行 worker 共享“当前文件”没有单一真值，本任务不引入虚假的路径标签。
- TUI/web：wire shape 不变；TUI 的 source progress 文案需在实现 diff 中复核语义，web sync command center 继续主要消费 source/stats。

## 4. Alternatives

### A. Planned replay denominator（已采用）

优点：分子分母同单位；反映实际慢工作；不会把 skipped 算作 scanned；无需新增字段。

代价：Claude/Codex 的 `SourceStarted.files_total` 语义由 inventory 变为本轮 replay，JSON/TUI 消费者需接受语义修正；SourceStarted 会在轻量 work-set 规划后才发出。

### B. Inventory denominator + pre-count skipped

优点：保留旧 `files_total` 语义，最终自然到 inventory total。

代价：大量 unchanged 文件会在解析前瞬间推进，之后只剩很短尾段；`files_scanned` 不再只表示 replay；现有“重放”标签失真。

### C. 扩展 SourceStarted 同时携带 inventory 与 planned

优点：语义最完整，UI 可显示“重放 X / 共 Y”。

否决建议：扩大 wire shape 与消费者测试，只为展示重复 stats 可在完成行提供的信息；超出最小完整修复。

### D. Per-file callback/channel send

否决：将 mpsc/闭包开销放进 worker 热路径，高文件数时会制造背压和大量 line/NDJSON 输出。

## 5. Compatibility And Failure Semantics

- 无 schema、数据库、CLI 参数或依赖变化。
- `SyncEvent` JSON 形状不变；仅 Claude/Codex `SourceStarted.files_total` 语义修正，必须同步 source-sync spec。
- worker/shard 失败时不 emit 虚假的最终完成；driver 维持现有错误传播。
- 通道满时进度快照可丢，最终 `SourceFinished` 仍走 await `send`；bar completion 兜底保留。
- detached worker/cancellation 行为不扩大；若实施需要改变它，必须回到规划。

## 6. Validation Design

- helper 单测使用受控 blocking tasks 证明：周期内只在 counter 增长时采样、能在 commit 前观察中间值、最终值单调且不超过 total。
- parser/driver 级测试收集真实 `SyncEvent`，分别锁定 Claude project planned total 与 Codex changed-file planned total。
- 渲染测试新增“最后一个 Progress 已到 len 时显示提交阶段，随后 SourceFinished 收敛”断言；`complete_active` 仍覆盖缺事件兜底。
- 保留 NDJSON、非 TTY、OpenCode spinner、TUI 与 source stats 回归。
- 性能对照使用相同 fixture/snapshot、release 构建与输出模式，至少 3 次，报告 median/range、RenderStats 与 progress drop；不得使用用户活动中的漂移数据直接比较。

## 7. Rollback

改动不涉及持久化。回滚 parser helper、SourceStarted 口径、文案和对应测试即可恢复旧事件语义；不需要数据迁移。
