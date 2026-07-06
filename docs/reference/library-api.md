# Library API

The crate exposes a small adapter surface for local desktop integrations and tests. It is still local-first: adapters read or mutate the same local SQLite runtime that the CLI uses.

## Stability contract

Prefer the root façade for embedding:

```rust
use llmusage::{
    AppPaths, Dashboard, JobRegistry, QueryFilter, ReportTimezone, Result, SourceKind, Store,
    SyncOptions,
};
```

The compatible stable set is:

- Runtime/store: `AppPaths`, `Store`, `BootstrapOptions`, `HolderKind`, `WorkerLock`
- Query: `Dashboard`, `QueryFilter`, `ReportTimezone`, dashboard payloads, Explorer payload/query types, logs payload/query types
- Sync jobs: `JobRegistry`, `SyncOptions`, `JobSnapshot`, `JobStatus`, `JobEvent`
- Domain/error: `SourceKind`, `LlmusageError`, `Result`
- Test helpers when `features = ["testing"]`: `Fixture`, `SeedEvent`

Broad modules such as `commands`, `parsers`, `integrations`, `runtime`, `web`, and `tui` remain public for the 0.7.x compatibility window, but they are implementation namespaces rather than the recommended adapter API. Future minor/major releases may move internals behind narrower modules after downstream callers migrate to the façade above.

## Open a store

```rust
use llmusage::{AppPaths, Result, Store};

fn open_store(root: std::path::PathBuf) -> Result<Store> {
    let paths = AppPaths::with_root(root)?;
    let store = Store::new(&paths)?;
    store.bootstrap()?;
    Ok(store)
}
```

Path resolution for CLI entrypoints is `--home <PATH>` first, then `LLMUSAGE_HOME`, then `~/.llmusage`.

## Dashboard queries

```rust
use llmusage::{
    Dashboard, ExplorerDimension, ExplorerFilters, ExplorerGranularity, ExplorerMetric,
    ExplorerQuery, QueryFilter, Result, Store,
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

`Dashboard::snapshot(&QueryFilter)` is the stable seam used by the web dashboard and static export. It includes the fixed dashboard sections plus the default Explorer payload. Use `Dashboard::explorer(&ExplorerQuery)` for custom Cost Explorer queries such as metric/group-by changes, Top N/Other, session filters, tool filters, and token component filters.

Behavior and Explorer queries use normalized `usage_turn` and `usage_tool_call` facts when a chosen metric or dimension needs them. They may report explicit `normalized`, `no_data`, `degraded`, or `unsupported` support states instead of pretending missing facts are zero.

## In-process sync jobs

`JobRegistry` provides start/get/cancel for in-process sync jobs. It is not a persistent job queue; restart recovery still comes from SQLite usage/cursor/run-log state.

CLI, hook, and library sync share the same `worker_lock`. Use `Store::acquire_worker_lock_with` when embedding custom sync paths.

## Testing fixture

Enable the feature in downstream tests:

```toml
[dev-dependencies]
llmusage = { path = "../llmusage", features = ["testing"] }
```

Then seed isolated data without touching a real user home:

```rust
let fixture = llmusage::testing::Fixture::new()?;
fixture.seed_dashboard(12)?;
let snapshot = llmusage::Dashboard::open(fixture.store())?.snapshot(&Default::default())?;
```
