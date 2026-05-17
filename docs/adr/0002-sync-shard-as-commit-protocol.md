# ADR 0002 — `SyncShard` 作为 commit 协议

- 状态：已采纳
- 落地阶段：阶段 2
- 落地日期：2026-05-06
- 相关代码：`src/store/mod.rs`（`SyncShard` / `ShardCommitStats`）、`src/store/sync_writer.rs`（`commit_shard`）、`src/parsers/{codex,claude,opencode}.rs`
- 相关术语：[SyncShard](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#7-syncshard) / [SyncRunWriter](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#8-syncrunwriter) / [Cursor](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#5-cursor) / [Bucket](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#6-bucket)

## 背景

阶段 2 之前，`SyncRunWriter` 的写入顺序协议是隐式的：每个 parser 自己写下面这串：

```rust
if !reset.is_empty() {
    writer.reset_file_events_batch(SourceKind::X, &reset)?;
}
for batch in events.chunks(EVENT_WRITE_BATCH_SIZE) {
    writer.write_event_batch(batch)?;
}
writer.write_cursor_batch(SourceKind::X, &cursors)?;
```

三个 parser（`parsers/codex.rs:107-137`、`parsers/claude.rs:101-132`、`parsers/opencode.rs:128-146`）各自抄一份。`reset_file_events_batch` / `write_event_batch` / `write_cursor_batch` 是 `pub` 方法，但调用顺序、batch size、计时器全靠 parser 自觉维护。`finish_sync_run` 是 no-op，证明协议本身是空壳。

`EVENT_WRITE_BATCH_SIZE` 是 `parsers/mod.rs::pub const`，所有 parser 都 import 它。

加一个新源 = 必须懂这个协议、必须正确实现"reset 在前 / event 分 chunk / cursor 在后 / write_ms 累加"，否则 bucket 会被重复计数。

## 决策

### 1. 引入 `SyncShard` 作为原子写入单元

`src/store/mod.rs`：
```rust
pub struct SyncShard {
    pub source: SourceKind,
    pub reset_path_hashes: Vec<String>,
    pub events: Vec<UsageEvent>,
    pub cursors: Vec<FileCursor>,
}

impl SyncShard {
    pub fn new(source: SourceKind) -> Self { ... }
}

pub struct ShardCommitStats {
    pub events_inserted: usize,
    pub write_ms: u64,
}
```

`SyncShard::new(source)` 是唯一构造器；不暴露 `Default`，强制 `source` 显式声明。

### 2. 单方法 `commit_shard` 固化协议

`src/store/sync_writer.rs`：
```rust
impl SyncRunWriter {
    pub fn commit_shard(&mut self, shard: SyncShard) -> Result<ShardCommitStats> {
        // reset → chunked write_event → cursor，原子；含 write_ms 计时
    }
}
```

`SyncRunWriter` 对外接口表面收敛到三个：
- `Store::begin_sync_run() -> SyncRunWriter`
- `commit_shard(shard) -> ShardCommitStats`
- `finish_sync_run()`

### 3. 内部细节降级为模块私有

- `reset_file_events_batch` / `write_event_batch` / `write_cursor_batch` 从 `pub fn` 降为 `fn`（不是 `pub(crate)`，也不留 `#[doc(hidden)]`）。
- `EVENT_WRITE_BATCH_SIZE` 从 `parsers/mod.rs::pub const` 移到 `sync_writer.rs::const`，模块私有。

Parser 不再 import 任何写入协议细节；不感知 batch size / 顺序 / 计时器。

### 4. Streaming 源用同一接口

OpenCode 是流式（page-by-page）：每页 events 一次 `commit_shard`，`reset_path_hashes` 与 `cursors` 传空，`OpencodeCursor` 仍由 `store.cursors().save_opencode_cursor` 自行收尾。流式 vs 批式在 writer 内部已被 `commit_shard` 抹平。

## 备选方案与否决理由

### 备选 A：保留 3 个细粒度 `pub fn`，仅文档说明顺序

否决：协议靠注释维持永远不可靠。第四个 parser 的实现者完全可能跳过 `reset_file_events_batch` 直接 `write_event_batch`，bucket 被双计数。Deletion-test 不通过：删去任何一个 fn 都让所有 parser 红，等价代价 = "全员同步改"。

### 备选 B：引入 `ParseOutcome { shards, stats_partial }` 中间类型

否决：当前 `SourceSyncStats` 已经包含 driver 需要的全部信息（`lock_wait_ms` 由 driver 后注入）。中间类型违反 YAGNI，且不通过 deletion test —— 删 `ParseOutcome` 让 stats 直接返回，调用面没有任何变化。

### 备选 C：把 `commit_shard` 拆成 `commit_reset` / `commit_events` / `commit_cursors`，按 builder 链式调用

否决：等价于把协议从 implicit 改成 chained-implicit。漏调任何一段仍然会破坏不变量（events 漏 cursor → 下次重复处理；reset 漏 events → 旧 events 残留）。一个原子方法把协议固化最深。

### 备选 D：cursors 在 sync 结束时一次性 batch 写

否决：阶段 2 之前 codex/claude 就这么做。改成"按 shard 提交"理由：
1. SQLite 三段事务（reset / events / cursor）原本就是相互独立的 UPSERT/INSERT-OR-IGNORE，按 shard 切不影响幂等。
2. codex/claude 一轮 sync 通常只有数个 shard，写次数略增（从 1 次 cursor batch 变 N 次），但绝对值 < 10，可忽略。
3. 换来"协议被一个方法完全封装"：parser 不再有"sync 结束前别忘了写 cursor"这个隐式 TODO。

## Deletion-test 论证

| 删什么 | 复发现象 | 是否更深 |
|------|---------|---------|
| 删 `commit_shard` | 三个 parser 重新抄 `if !reset.is_empty() { reset_file_events_batch(...) } / for batch in events.chunks(EVENT_WRITE_BATCH_SIZE) { write_event_batch(...) } / write_cursor_batch(...)` 三段；每个 parser 重新 import `EVENT_WRITE_BATCH_SIZE`；codex.rs / claude.rs / opencode.rs 各自累加 `write_started = Instant::now()` / `write_ms` | ✅ |
| 把 3 个内部 fn 重新升回 `pub` | 调用面不变，但协议保护伞失效；新 parser 可以绕过 `commit_shard` 走单段写入，bucket 双计数风险回归 | ✅ |
| 把 `EVENT_WRITE_BATCH_SIZE` 移回 `parsers/mod.rs::pub const` | parser 重新感知 batch 细节；改 batch size = 改公共 API | ✅ |
| 删 `SyncShard::new` 强制构造器，改成裸结构体字面量 | 调用方可以省略 `source`（取 `Default`），静默走错 SQL key | ✅（间接：通过类型检查阻止 footgun） |

## 后果

- 三个 parser 都从"读 fixture → 转 events → 自己写入"压到"读 fixture → 转 events → push 到 SyncShard → commit_shard"。新增第四个 parser 时实现者只需要构造 `SyncShard`，不接触 SQLite。
- writer 内部以后改 batch size / 写入顺序 / cursor SQL 不会触动任何 parser。
- `tests/sync_regression.rs`（23.8K 行，覆盖三源 append / replace / inode-rotate）继续作为安全网；新增 `commit_shard_runs_reset_then_events_then_cursor` 单测在内存 store 上验证 reset 顺序、bucket 一致性、cursor 落库。
- 阶段 3 的 `SourceParser` trait 统一签名 `(store, writer, parallelism) -> SourceSyncStats` 是这条决策的直接收益：流式 vs 批式差异已在 writer 内部抹平。

## 验证

- 阶段 2 完成时：`rtk cargo build` / `cargo fmt --check` / `clippy -D warnings` / `cargo test --test-threads=1` 全绿（35/35 测试）。
- 新单测：`store::sync_writer::tests::commit_shard_runs_reset_then_events_then_cursor` 在 `TempDir` 内 store 上 seed 1 个 event → reset 同 path_hash → 写 5 个新 events + 1 个 cursor，断言 reset 在前（`event_count == 5`）、bucket 总 tokens=150、cursor 落库。
- 安全网：`tests/sync_regression.rs` 6 个测试通过——三源 append / replace / inode-rotate 路径未回归。

## 0.6.x 更新：行为事实作为 shard 附属事实

0.6.x 为支持 Dashboard Activity / Tools / Optimize / Compare，在 `SyncShard` 上追加 `turns: Vec<UsageTurn>` 与 `tool_calls: Vec<UsageToolCall>`，并在 `ShardCommitStats` 上追加 `turns_inserted` / `tool_calls_inserted`。

这不改变 ADR 的核心决策：parser 仍只构造 shard，不能直接写 SQLite；`SyncRunWriter::commit_shard` 仍是唯一写入协议入口。新增行为事实的约束是：

- `usage_event` / `usage_bucket_30m` 仍是成本与用量主路径，行为事实不能污染 bucket 语义。
- `turns` / `tool_calls` 可为空，source 不支持工具级证据时由 query/API 显式返回 no_data / degraded。
- 对同一 `source_path_hash` 做 reset 时，writer 同时清理旧 `usage_turn` / `usage_tool_call`，再写入新 facts，保持整文件重放幂等。
- `UsageToolCall.safe_preview` 只能保存 bounded 安全摘要；完整 prompt、assistant text 和文件内容不得进入行为事实表。

验证补充：

- `store::sync_writer::tests::commit_shard_writes_behavior_facts_and_resets_them_by_source_path`
- `store::migrations::tests::migration_v11_creates_behavior_fact_tables`
