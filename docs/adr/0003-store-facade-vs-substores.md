# ADR 0003 — `Store` façade 与 5 个借用 view

- 状态：已采纳
- 落地阶段：阶段 5
- 落地日期：2026-05-06
- 相关代码：`src/store/mod.rs`、`src/store/{cursor,integration,run_log,sync_status,trigger}.rs`
- 相关术语：[Store](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#9-store) / [RunLog](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#11-runlog) / [Cursor](https://github.com/bahayonghang/llmuasage/blob/main/CONTEXT.md#5-cursor)

## 背景

阶段 5 之前的 `Store` 是 god-object：表面上 `src/store/mod.rs` 只声明 195 行，实际上 façade 在 7 个子模块各自的 `impl Store` 里平摊出 25+ 个 `pub fn`：

```text
connection.rs  : open_connection / reset_open_connection_counter / open_connection_count
schema.rs      : new / bootstrap / reset_usage_data
cursor.rs      : load_file_cursors / save_file_cursors / load_opencode_cursor / save_opencode_cursor
integration.rs : record_integration_state / load_integration_states / load_integration_states_with_conn
lease.rs       : acquire_worker_lock / release_worker_lock / recover_stale_lease
run_log.rs     : record_run_start / finish_run / recover_running_runs / recent_runs / recent_runs_with_conn
sync_status.rs : save_source_sync_statuses / load_source_sync_statuses
trigger.rs     : record_trigger / load_triggers
```

调用方需要 `store.record_run_start(...)`、`store.load_file_cursors(...)`、`store.upsert_trigger_state(...)` —— 25+ 个方法在同一个 façade 上扁平展开，没有子领域语义。新增子领域时所有 caller 又看到全表面。

## 决策

### 1. 拆 5 个借用 view

```rust
pub struct CursorStore<'a> { store: &'a Store }
pub struct IntegrationStateStore<'a> { store: &'a Store }
pub struct RunLog<'a> { store: &'a Store }
pub struct SyncStatusStore<'a> { store: &'a Store }
pub struct TriggerStore<'a> { store: &'a Store }
```

5 个 view 全部用 `pub struct XxxStore<'a> { store: &'a Store }` 借用形态。每个 view 通过 `XxxStore::new(store)`（pub(super) 限定）构造。原 `impl Store { fn ... }` 整段迁到 `impl<'a> XxxStore<'a> { fn ... }`，方法体内 `self.open_connection()` 改成 `self.store.open_connection()`。

### 2. view-getter 集中暴露在 `store/mod.rs`

```rust
impl Store {
    pub fn cursors(&self) -> CursorStore<'_>;
    pub fn integration_state(&self) -> IntegrationStateStore<'_>;
    pub fn run_log(&self) -> RunLog<'_>;
    pub fn sync_status(&self) -> SyncStatusStore<'_>;
    pub fn triggers(&self) -> TriggerStore<'_>;
}
```

5 个 view-getter 集中在 `store/mod.rs` 顶部 `impl Store {}` 块——façade 入口集中暴露，看一眼就能列出所有子领域。同时 `pub use cursor::CursorStore;` 等 re-export 让 caller 写具体类型。

### 3. façade 自身能力最小化

`Store` façade 上保留的方法：
- `Store::new(paths)`
- `Store::open_connection()` / `Store::reset_open_connection_counter()` / `Store::open_connection_count()`
- `Store::acquire_worker_lock()` / `Store::release_worker_lock()` / `Store::recover_stale_lease()`
- `Store::bootstrap()` / `Store::reset_usage_data()`
- `Store::begin_sync_run()`
- 5 个 view-getter

其他原 `Store::xxx` 全部迁到对应 view。规划文档原本提的"`recover_running_runs` 留在 Store"被否决——它属于 `run_log` 子领域，留在 `RunLog<'a>` 上更深。

### 4. 方法名留旧

调用方写 `store.run_log().recent_runs()` 而不是 `store.run_log().recent()`。`load_file_cursors` / `record_integration_state` / `record_run_start` / `load_source_sync_statuses` / `upsert_trigger_state` 等 18 个方法名一字未改。

### 5. `_with_conn` 变体保持 `pub(crate)`

`load_integration_states_with_conn` / `recent_runs_with_conn` 仅 `query/mod.rs::Dashboard::health` 使用，可见性不变。

## 备选方案与否决理由

### 备选 A：保留 god-object，仅做模块内私有方法整理

否决：模块内整理不影响 façade 表面。25+ 方法仍然挤在一处；新增子领域（例如 `lease_state`）时所有 caller 又看到全表面。Deletion-test 不通过：删模块不影响 façade。

### 备选 B：view 持 `Arc<Store>` 而非借用

否决：22 处 caller 都是 `store.<view>().method()` 临时借用，跨 await 自然不持有；改成 `Arc<Store>` 会让每次 view 构造都增加引用计数原子操作，没有收益。`Store: Clone` 仍然成立（`AppPaths: Clone`，路径字符串拷贝），不需要再加 `Arc` 一层。

### 备选 C：view 持 owned `Store`

否决：`store.cursors().load_file_cursors(...)` 共 22 处调用面 = 22 次 `Store::clone()`。即便 clone 只是路径字符串拷贝，cascade 起来违反"借用优先"。Stage 1 的 `Dashboard` 持 owned `Store` 是 web handler async 跨 await 的特例；view 是同步借用场景，不需要。

### 备选 D：方法名缩短（`load_file_cursors` → `load_files`、`record_integration_state` → `record`）

否决：deepness 已经体现在 view 命名空间分组上。`store.run_log().recent_runs()` 比 `store.recent_runs()` 多一层语义，缩名（`store.run_log().recent()`）反而失去"recent_runs 是什么"的自描述。同时改名让调用面改动从 N 处变 2N 处（视图入口 + 方法名），单点风险翻倍。

### 备选 E：把 5 个 view-getter 分散到各自模块的 `impl Store {}` 块

否决：拆分原本是为了"看到一处就知道 façade 长什么样"。5 个 view-getter 集中在 `store/mod.rs` 顶部 `impl Store {}` 让"Store 是 façade、view 是它的子领域入口"的角色一目了然；分散到子模块 = 入口面散开，违反 deletion test（删 `store/mod.rs::cursors` 和删 `store/cursor.rs::impl Store::cursors` 等价代价不同）。

### 备选 F：把 `lease.rs` 也拆成 `LeaseStore<'a>` view

否决：`acquire_worker_lock` 返回 `WorkerLock` guard，guard drop 时 `release_worker_lock`；guard 持有 `Store` owned，借用 view 模式不适用。`lease` 的语义是"façade 自身的并发能力"，不是"持久化数据访问"，留在 `Store` 主体合适。

## Deletion-test 论证

| 删什么 | 复发现象 | 是否更深 |
|------|---------|---------|
| 删 5 个 view + 5 个 view-getter | 22 处 caller 必须重新挤回 `Store` 单 façade 上的 18 个 `pub fn`；Store::method 数量从 6 + 5 view-getter 回到 24+；新增 `lease_state` 子领域时 caller 又看到全表面，god-object 复发 | ✅ |
| 改成 owned `Store` 而非借用 | 22 处 caller 各自 `Store::clone()`；cascade clone 复发；Store: Clone 是为局部 async 跨 await，不应被全表面分摊 | ✅ |
| 把 view-getter 分散到子模块 `impl Store {}` | 想列出所有子领域必须 grep 5 个文件；`store/mod.rs` façade 顶部不再是单一入口；维护者每次拆 view 都要在 mod.rs / 子模块 两处加方法 | ✅ |
| 缩短方法名（`recent_runs` → `recent`） | 22 处 caller 全改；语义损失（"recent" 不知道指什么）；调用面改动 2 倍代价 | ✅（保留旧名更深） |

## 后果

- `Store` 上 18 个混合表面拆成 5 个 view + 6 个 façade 自身能力。新增子领域时只在对应 view 文件加方法，caller 通过 `store.<view>().new_method()` 拿到；其他 view 的 caller 完全不受影响。
- view 用借用形态后，`Store` 作为 `Clone` 的合理性反而被印证：caller 通过 view 拿临时 `&Store`，跨 await 时直接 clone 一次 `Store`（路径字符串拷贝）即可；不需要 `Arc<Store>`。
- 22 处 caller 切换为 `store.<view>().method()` 形式：parsers / integrations / query / 8 个 commands / 2 个 tests。一次性切完，无残留旧 API。
- `Dashboard::health` 内部继续复用单一 `Connection`：通过保留 `pub(crate)` 的 `_with_conn` 变体支持。可见性不变。

## 验证

- 阶段 5 完成时：`rtk cargo build` / `cargo fmt --check` / `clippy -D warnings` / `cargo test --test-threads=1` 全绿（35/35 测试）。
- `tests/sync_regression.rs` 6 个测试通过——三源 append / replace / inode-rotate 路径未回归。
- `tests/local_flow.rs::local_flow_installs_syncs_exports_and_uninstalls` 通过——init/sync/export html/uninstall 端到端流程不变。
- `dashboard_snapshot_uses_single_connection_and_matches_individual_methods` 继续通过——Dashboard 单连接断言未受 view 切换影响。
