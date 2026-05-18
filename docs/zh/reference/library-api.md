# 库 API

crate 暴露了一组较小的适配层接口，用于本地桌面集成和测试。它仍遵循本地优先边界：适配层读写的是 CLI 使用的同一个本地 SQLite 运行时。

## 打开 Store

```rust
use llmusage::{paths::AppPaths, store::Store, Result};

fn open_store(root: std::path::PathBuf) -> Result<Store> {
    let paths = AppPaths::with_root(root)?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok(store)
}
```

CLI 入口的路径解析顺序是 `--home <PATH>`、`LLMUSAGE_HOME`、`~/.llmusage`。

## Dashboard 查询

```rust
use llmusage::{Dashboard, QueryFilter, Result, Store};

fn load_dashboard(store: &Store) -> Result<()> {
    let filter = QueryFilter::default();
    let dashboard = Dashboard::open(store)?;
    let _snapshot = dashboard.snapshot(&filter)?;
    let _activity = dashboard.activity_breakdown(&filter)?;
    let _tools = dashboard.tool_breakdown(&filter)?;
    let _optimize = dashboard.optimize(&filter)?;
    let _compare = dashboard.model_compare(&filter, None, None)?;
    Ok(())
}
```

`Dashboard::snapshot(&QueryFilter)` 是 Web Dashboard 和静态导出的稳定 seam。行为查询使用 `usage_turn` 与 `usage_tool_call`，并可能返回显式 degraded/no-data 支持状态。

## 进程内 sync jobs

`JobRegistry` 提供进程内 sync job 的 start/get/cancel。它不是持久 job 队列；重启后的可恢复状态仍来自 SQLite 中的 usage、cursor、source-file diagnostics 和 run log。

CLI、hook、library sync 共用同一把 `worker_lock`。嵌入自定义 sync 路径时使用 `Store::acquire_worker_lock_with`。

## 测试 fixture

下游测试中启用 feature：

```toml
[dev-dependencies]
llmusage = { path = "../llmusage", features = ["testing"] }
```

然后用隔离数据，不触碰真实用户 home：

```rust
let fixture = llmusage::testing::Fixture::new()?;
fixture.seed_dashboard(12)?;
let snapshot = llmusage::Dashboard::open(fixture.store())?.snapshot(&Default::default())?;
```
