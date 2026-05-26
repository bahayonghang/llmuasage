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
use llmusage::{
    store::Store, Dashboard, ExplorerDimension, ExplorerFilters, ExplorerGranularity,
    ExplorerMetric, ExplorerQuery, QueryFilter, Result,
};

fn load_dashboard(store: &Store) -> Result<()> {
    let filter = QueryFilter::default();
    let dashboard = Dashboard::open(store)?;
    let _snapshot = dashboard.snapshot(&filter)?;
    let _activity = dashboard.activity_breakdown(&filter)?;
    let _tools = dashboard.tool_breakdown(&filter)?;
    let _optimize = dashboard.optimize(&filter)?;
    let _compare = dashboard.model_compare(&filter, None, None)?;
    let _explorer = dashboard.explorer(&ExplorerQuery {
        filter,
        granularity: ExplorerGranularity::Day,
        metric: ExplorerMetric::AttributedCostUsd,
        group_by: ExplorerDimension::Session,
        filters: ExplorerFilters {
            tool_name: Some("Read".to_string()),
            ..Default::default()
        },
        limit: 8,
        include_other: true,
    })?;
    Ok(())
}
```

`Dashboard::snapshot(&QueryFilter)` 是 Web Dashboard 和静态导出的稳定 seam。它包含固定 Dashboard 区块和默认 Explorer payload。自定义 Cost Explorer 查询使用 `Dashboard::explorer(&ExplorerQuery)`，用于切换 metric/group-by、Top N/Other、session 筛选、tool 筛选和 token component 筛选。

行为查询和 Explorer 在所选指标或维度需要时会使用标准化 `usage_turn` 与 `usage_tool_call` facts。它们会返回显式 `normalized`、`no_data`、`degraded` 或 `unsupported` 支持状态，而不是把缺失事实伪装成 0。

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
