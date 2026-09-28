# Rust API 迁移到 2.0

2.0.0 为仓库 `v1.2.0` 发布后已有的公开 Rust API 变化声明主版本边界。
这些变化支持独立的 Antigravity CLI/IDE 与 Pi/OMP 来源、远端主机、
来源上报成本和配额缓存来源。2.0.0 开发版本不代表已经发布。

下游 crate 需要重新编译，并检查受影响的构造式和 match。正式门禁保持
`cargo semver-checks --baseline-rev v1.2.0`。主版本检查通过表示声明版本
允许这些 API 变化，不代表恢复 v1.2.0 源码兼容性。

## 查询与主机范围

`ReportFilter` 现在包含 `QueryFilter`。把 `since`、`until`、`timezone`、
`source` 放入 `filter`。Deref 字段访问不能保留旧 struct 字面量。
例如，构造仅查询本机的 UTC 报表：

```rust
use llmusage::{QueryFilter, ReportTimezone};
use llmusage::query::reports::ReportFilter;
use llmusage::store::LOCAL_HOST_ID;

let report = ReportFilter {
    filter: QueryFilter {
        host_id: Some(LOCAL_HOST_ID.to_owned()),
        timezone: ReportTimezone::Utc,
        ..Default::default()
    },
    ..Default::default()
};
```

`QueryFilter.host_id = None` 不限制主机。本机操作使用 `LOCAL_HOST_ID`，
远端操作使用实际内部 host ID；显示标签不能代替内部 ID。
`ReportCommonArgs::to_filter` 现在需要 Store 来解析主机参数。

以下方法增加了 Store 或 host 参数。表中省略 `&self`；参数顺序必须保持。

| 所属类型 | 当前调用 |
| --- | --- |
| `ReportCommonArgs` | `to_filter(&store, project)` |
| `SourceFileStore` | `counts(source, host_id)` |
| `SourceFileStore` | `tracked_paths(source, host_id)` |
| `SourceFileStore` | `lossy_rebuild_risk(source, host_id)` |
| `SourceFileStore` | `sweep_missing(source, host_id, run_started_at)` |
| `SourceFileStore` | `mark_inventory_seen(source, host_id, file_paths, seen_at)` |
| `SyncStatusStore` | `load_source_sync_statuses(host_id)` |
| `SyncStatusStore` | `save_source_sync_statuses(host_id, statuses)` |
| `SyncStatusStore` | `mark_recent_completed(source, host_id, at)` |
| `Store` | `reset_for_source(source, host_id)` |
| `Store` | `mark_source_file_deleted(source, host_id, file_path)` |
| `CursorStore` | `load_file_cursors(source, host_id)` |
| `CursorStore` | `load_opencode_cursor(host_id)` |
| `CursorStore` | `load_zcode_cursor(host_id)` |
| `CursorStore` | `save_opencode_cursor(host_id, cursor)` |
| `CursorStore` | `save_zcode_cursor(host_id, cursor)` |

## 公开结构体

完整 struct 字面量需要补充下列字段。仅当类型实现 `Default` 且默认值
符合调用目的时，使用 `..Default::default()`。

| 结构体 | 新增字段 |
| --- | --- |
| `JsonlRecord` | `line_number`, `durable` |
| `DashboardSnapshot` | `hosts` |
| `DashboardCoreSnapshot` | `hosts` |
| `DashboardInteractiveSnapshot` | `hosts` |
| `SourceCapabilityStatus` | `accounting` |
| `ReportFilter` | `filter` |
| `SyncShard` | `host_id`, `host_prefix_applied`, `opencode_cursor`, `zcode_cursor` |
| `HomeOverviewSeriesItem` | `antigravity_ide` |
| `TopSessionRow` | `first_event_at`, `last_event_at` |
| `QueryFilter` | `host_id` |
| `ReportCommonArgs` | `host` |
| `UsageEvent` | `source_cost` |
| `DriveContext` | `sweep_host_ids` |
| `LossyRebuildRisk` | `host_id` |

`JsonlRecord.line_number` 从 1 开始。仅当 `end_offset` 是换行终止的
持久边界时才设置 `durable`。cursor 与库存必须保持来源和主机范围。
来源给出权威成本时保留 `source_cost`。这些字段不能随意补默认值。

`LossyRebuildRisk` 包含 owned host ID，因此不再实现 `Copy`。
调用方改用借用或显式 clone。

## 枚举与入口

穷尽 match 需要处理以下变化：

| 枚举 | 新增 variant 或字段 |
| --- | --- |
| `SourceKind` | `AntigravityIde`, `Omp` |
| `PricingStatus` | `SourceReported` |
| `SyncEvent` | `RemoteHostStarted`, `RemoteHostFinished`, `RemoteHostSkipped` |
| `Commands` | `AntigravityIde`, `Remote`；`Sync` 增加 `emit_shards`, `since` |

`antigravity` 与 `antigravity_ide` 保持独立；`pi` 与 `omp` 保持独立。
枚举声明顺序及数值转换已经变化，涉及 `PricingStatus::Unpriced`、
后续 `SyncEvent` variant 和后续 `SourceKind` variant。`SourceKind`
派生排序也已变化。持久化和传输使用文档定义的 source ID；
下游需要重新检查数值转换和自定义排序。

| 原 API | 当前 API |
| --- | --- |
| `parsers::PiParser` / `parsers::pi::PiParser` | Pi 使用 `parsers::PiFormatParser::pi()`；OMP 使用 `::omp()` |
| `PricingCatalog::static_v1()` | `PricingCatalog::embedded()`，返回 `&'static PricingCatalog` |
| `commands::tui::run(app)` | `commands::dash::run(app, false).await` |

同时扫描 Pi 和 OMP 的调用方需要注册两个实例。CLI 的 `tui` 别名
保持现有弃用提示行为。

## 订阅配额结果

`subscription::fetch_all(&context, bypass_cache).await` 现在返回
`UsageFetchOutcome { report, cache_hit }`。需要原 `UsageFetchReport`
时读取 `.report`；需要展示缓存来源时读取 `cache_hit`。
缓存文档仍只存 `UsageFetchReport`。

## 数据与同步安全

crate 主版本调整不改变 SQLite schema version、来源 accounting version
或远端 wire version。各数据边界保留原有检查。旧二进制可能通过
`SchemaTooNew` 拒绝较新数据库；crate 版本变化不保证数据库可降级。

普通 sync 保留旧核算历史，并跳过受影响来源的写入。修复仍需显式执行
`llmusage sync --rebuild --source <source>`。修复前先恢复缺失来源文件。
仅当接受清除无法重建的历史时才添加 `--allow-lossy-rebuild`。
参见[本地数据安全](../safety/)。
