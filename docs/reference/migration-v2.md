# Migrate the Rust API to 2.0

Version 2.0.0 declares the public Rust API changes made since the repository's
`v1.2.0` release. The changes support separate Antigravity CLI/IDE and Pi/OMP
sources, remote hosts, source-reported costs, and quota cache provenance.
The 2.0.0 development version is not a release announcement.

Recompile downstream crates and review each affected constructor and match.
The formal check remains `cargo semver-checks --baseline-rev v1.2.0`. A pass
for the major version means the declared version permits the API changes.
The pass does not establish source compatibility with v1.2.0.

## Queries and host scope

`ReportFilter` now contains a `QueryFilter`. Move `since`, `until`, `timezone`,
and `source` into `filter`. Deref field access does not preserve old struct
literals. For example, construct a local UTC report with:

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

`QueryFilter.host_id = None` applies no host filter. Use `LOCAL_HOST_ID` for
local-only work and the actual internal host ID for remote work. Host labels
are not internal IDs. `ReportCommonArgs::to_filter` now needs the store to
resolve host arguments.

The following methods require new store or host arguments. The table omits
`&self`; argument order is significant.

| Owner | Current call |
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

## Public structs

Update complete struct literals for these added fields. Use
`..Default::default()` only when the type implements `Default` and the default
values match the caller's intended behavior.

| Struct | Added fields |
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

`JsonlRecord.line_number` is one-based. Set `durable` only when `end_offset`
is a newline-terminated boundary. Preserve the source and host scope of
cursors and inventory. Preserve `source_cost` when the source supplies an
authoritative cost. Do not fill these fields with arbitrary defaults.

`LossyRebuildRisk` contains an owned host ID and no longer implements `Copy`.
Borrow the value or clone it explicitly.

## Enums and entry points

Update exhaustive matches for these variants:

| Enum | Added variants or fields |
| --- | --- |
| `SourceKind` | `AntigravityIde`, `Omp` |
| `PricingStatus` | `SourceReported` |
| `SyncEvent` | `RemoteHostStarted`, `RemoteHostFinished`, `RemoteHostSkipped` |
| `Commands` | `AntigravityIde`, `Remote`; `Sync` adds `emit_shards`, `since` |

Keep `antigravity` and `antigravity_ide` distinct. Keep `pi` and `omp` distinct.
Enum declaration order and numeric casts have changed, including
`PricingStatus::Unpriced`, later `SyncEvent` variants, and later `SourceKind`
variants. `SourceKind` derived ordering has also changed. Use documented
source IDs for persistence and transport. Recheck any downstream numeric
casts or custom sorting.

| Previous API | Current API |
| --- | --- |
| `parsers::PiParser` / `parsers::pi::PiParser` | `parsers::PiFormatParser::pi()` for Pi; `::omp()` for OMP |
| `PricingCatalog::static_v1()` | `PricingCatalog::embedded()`, which returns `&'static PricingCatalog` |
| `commands::tui::run(app)` | `commands::dash::run(app, false).await` |

Register both Pi parser instances when the caller must scan both products.
The CLI `tui` alias retains its existing deprecation behavior.

## Subscription quota results

`subscription::fetch_all(&context, bypass_cache).await` now returns
`UsageFetchOutcome { report, cache_hit }`. Use `.report` where the caller
needs the previous `UsageFetchReport`; use `cache_hit` to show cache
provenance. Cache documents still store `UsageFetchReport` only.

## Data and sync safety

The crate major version does not change the SQLite schema version, source
accounting versions, or remote wire version. Each data boundary keeps its
existing checks. Older binaries can reject a newer database with
`SchemaTooNew`; a crate version change does not make database downgrades safe.

Ordinary sync preserves legacy-accounting history and skips writes for the
affected source. Repair remains the explicit command
`llmusage sync --rebuild --source <source>`. Restore missing source files
before repair. Add `--allow-lossy-rebuild` only when accepting the loss of
history that cannot be rebuilt. See [local data safety](../safety/).
