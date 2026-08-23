# ADR 0005 — JobRegistry 内存态任务编排

- 状态：拟稿（0.5.0 sprint M0- 雏形 / M2 完整）
- 落地阶段：M0- 雏形 / M2 完整
- 落地日期：TBD
- 相关代码：`src/sync/{job_registry,default,engine}.rs`、`src/web/mod.rs::api_jobs`、`src/tui/sync_control.rs`
- 相关术语：Job / JobRegistry / JobSnapshot（见仓库根目录 CONTEXT.md）
- 关联 PRD：llmusage-integration-prd-v1.1.md §F0.2（D4，仓库根目录）

## 背景

ccr-ui Tauri 命令期望的同步语义是：

```text
job_id = start_usage_import_job({recent_days, rebuild, source})
loop {
    snapshot = get_usage_import_job(job_id)
    if snapshot.status in {completed, failed, cancelled}: break
    sleep 500ms
}
```

`run_with_progress` 把进度推到 `mpsc::Sender<SyncEvent>`，是 in-process push。Tauri 命令是无状态 RPC，每次 `get_usage_import_job` 都是新调用，**不能直接持有 receiver**。

需要在 push 与 poll 之间架一层登记表。

## 决策

### 1. 内存 only DashMap

```rust
pub struct JobRegistry {
    inner: Arc<DashMap<JobId, Arc<Mutex<JobState>>>>,
}
```

进程退出即清空。理由：

1. sync 中途崩溃后 job 状态本就不可信，恢复语义模糊。
2. ccr-ui Tauri 重启后用户重发 import 与从 SQLite 续接 job 同样代价。
3. 避免引入"重启后未完成 job 怎么办"的状态机分支。
4. WAL 锁释放与 Drop 天然对齐。

### 2. JobState 内部数据流

```rust
struct JobState {
    snapshot: JobSnapshot,                     // 暴露给 snapshot()
    cancel:   CancellationToken,
    handle:   Option<JoinHandle<()>>,
}
```

`start()` 起 tokio task 跑 `run_with_progress`，task 内部持有 `mpsc::Receiver<SyncEvent>` 把 event 翻译成 `JobSnapshot` 字段更新。

### 3. snapshot() 永远返回 clone，不持锁

```rust
pub fn snapshot(&self, id: &str) -> Option<JobSnapshot> {
    self.inner.get(id).map(|e| e.lock().unwrap().snapshot.clone())
}
```

锁颗粒：每个 job 一个 mutex，互不相关。

### 4. 取消语义

`JobRegistry::cancel(id)` → `state.cancel.cancel()`。`run_with_progress` 在文件边界检查后退出，emit `SyncEvent::Cancelled`，registry 把 status 设为 `cancelled`。已写数据保留，cursor 不回滚。

### 5. list_recent 容量上限

DashMap 不无限增长。`list_recent` 默认上限 50 条；超过时按 `finished_at` 老的先 evict。运行中 job 永不 evict。

### 6. 输入校验与 bounded recent

JobRegistry 不解释 transport 字符串。CLI、Web 与 public `try_start` 都通过 `ValidatedSyncRequest` 校验 source、`recent_days` 与 parallelism；非法输入在创建 job 前返回稳定错误码。`recent_days` 使用单一 UTC cutoff：文件型来源按事件时间过滤，OpenCode 在 SQL 层裁剪旧行。bounded run 不写 full-history cursor 或整文件 reset，因此后续 full sync 仍可恢复窗口外历史。

## 备选方案与否决理由

### 备选 A：SQLite job 表持久化

`job(id, status, started_at, ...)`，重启后 `recover_jobs()` 把 `running` 翻为 `aborted`。否决：

1. 持久化 job 状态后，"重启续接 job"成为隐含承诺；实现上 sync 已死，重启只能展示"上次失败了"，对用户无价值。
2. SQLite 写竞争：sync 写量级大，job 状态高频更新会拖累 sync 写吞吐。
3. WAL 文件膨胀。

### 备选 B：milestone 持久化（混合）

仅 Started/RecentReady/Finished/Failed 四个里程碑写 SQLite，Progress 内存。否决：双写一致性边界条件多（崩溃在 milestone 之间状态如何标记）。

### 备选 C：直接暴露 mpsc::Receiver 给 Tauri

让 Tauri 命令持有 receiver。否决：Tauri 命令是无状态的，`#[tauri::command]` 函数不能跨调用持有 Rust 对象。要走 `tauri::State` 又回到 registry 模式。

## Deletion-test 论证

如果删除 `JobRegistry` → ccr-ui 不能轮询 sync 状态 → 用户体验退化为"点击导入后无反馈"。这与现有 ccr-ui v1 行为相同（V1 用 ccr-store 同步阻塞调用），但 0.5.0 既然要 push 进度事件，registry 是兑现承诺的最小代价。

## 后果

正面：

- ccr-ui 适配层薄：`start → poll → cancel` 三命令各对应 registry 一调用。
- 没有 SQLite 写入压力；不影响 sync 性能。
- 进程崩溃语义清晰（任何运行中的 job 蒸发，与下次重启无关）。

负面：

- llmusage CLI `--json-events` 子进程模式不能复用 registry（子进程退出即丢）。该模式只能 NDJSON 流式 push 给调用者，由调用者维护 job 表。文档要明示。
- 多 ccr-ui 实例共享 `~/.ccr/llmusage/` 时，job_id 是进程内的，不能在另一进程查到。`with_root` 隔离子目录可以缓解，但需要 ccr-ui 自己保证。

## 验证

- 单测：`registry_returns_running_snapshot_immediately_after_start`
- 单测：`registry_marks_completed_after_finished_event`
- 单测：`registry_cancel_propagates_to_run_with_progress`
- 单测：`list_recent_evicts_oldest_finished`
- 集测：`tauri_command_poll_loop_observes_full_lifecycle`

## 1.3 更新：默认执行器归属 sync 应用层

`SyncExecutor` 的默认 concrete implementation 已从 CLI 命令模块迁移为
`src/sync/default.rs::DefaultSyncExecutor`，三阶段流水线位于
`src/sync/engine.rs`。`JobRegistry::default()` 与该实现同层组合；Web/TUI
直接使用默认 registry，测试仍可通过 `JobRegistry::new(Arc<dyn
SyncExecutor>)` 注入 executor。

`commands::sync::CommandSyncExecutor` 仅作为兼容 re-export 保留，旧的
`run_once*`/`run_store_once*` 路径也只委托给 sync engine。这样删除 CLI
adapter 不会再删除同步策略、重建/修复、remote 生命周期或 registry 的默认
运行能力。
