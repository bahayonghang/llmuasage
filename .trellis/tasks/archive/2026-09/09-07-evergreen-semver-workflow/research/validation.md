# Local validation (2026-09-07)

Working directory: `D:\Documents\Code\CLI\llmusage`.
`cargo-semver-checks` 0.50.0 was on PATH via the isolated scratch `--root`.

## Commands and exit codes

| Command | Exit |
| --- | ---: |
| `git rev-parse "v1.2.0^{commit}"` | 0 |
| `cargo semver-checks --help` | 0 |
| `cargo semver-checks --baseline-rev v1.2.0` | 100 |
| `python scripts/check-ci-gate.py --self-test` | 0 |
| `python scripts/check-ci-gate.py` | 0 |
| `cargo metadata --locked --no-deps --format-version 1` | 0 |
| `cargo doc --locked --no-deps` | 0 |

`git rev-parse` output: `9b7a6f3dec12764222891c2d8f5aeb42db7bd490`.

The semver command cloned git tag `v1.2.0`, built current `llmusage v1.3.0`
and baseline `llmusage v1.2.0`, then ran API comparison:

`Checking llmusage v1.2.0 -> v1.3.0 (minor change)`
`Checked 196 checks: 184 pass, 11 fail, 1 warn, 58 skip`

The log does not mention crates.io, `openrijal/llmusage`, `--locked`, or a
skip of the comparison. Full log: `research/semver-baseline.log`.

Exit 100 is cargo-semver-checks reporting real SemVer failures. This task
records them. It does not add a shim, wrap the failure, downgrade the
check, or bump `Cargo.toml` version (still `1.3.0`).

GitHub PR / main / `workflow_dispatch` check-runs: UNVERIFIED (push
forbidden). Local workflow text plus the local command run is the
acceptance bar for that remote clause.

## Unique SemVer diffs (1.3.0 vs 1.2.0)

Tool summary: `semver requires new major version: 11 major and 0 minor
checks failed`; `1 major` warning.

### constructible_struct_adds_field

- `LossyRebuildRisk.host_id`
- `DashboardSnapshot.hosts`
- `UsageEvent.source_cost`
- `DriveContext.sweep_host_ids`
- `JsonlRecord.line_number`
- `JsonlRecord.durable`
- `QueryFilter.host_id`
- `ReportCommonArgs.host`
- `TopSessionRow.first_event_at`
- `TopSessionRow.last_event_at`
- `SyncShard.host_id`
- `SyncShard.host_prefix_applied`
- `SyncShard.opencode_cursor`
- `SyncShard.zcode_cursor`
- `DashboardCoreSnapshot.hosts`
- `DashboardInteractiveSnapshot.hosts`
- `ReportFilter.filter`

### derive_trait_impl_removed

- `LossyRebuildRisk` no longer derives `Copy`

### enum_no_repr_variant_discriminant_changed

- `PricingStatus::Unpriced` 2 -> 3
- `SyncEvent::Finished` 16 -> 19
- `SyncEvent::Failed` 17 -> 20
- `SyncEvent::Cancelled` 18 -> 21
- `SourceKind::Grok` 6 -> 7
- `SourceKind::Zcode` 7 -> 8
- `SourceKind::DeepseekHarness` 8 -> 9

### enum_struct_variant_field_added

- `Commands::Sync.emit_shards`
- `Commands::Sync.since`

### enum_variant_added

- `SourceKind::Omp`
- `Commands::Remote`
- `SyncEvent::RemoteHostStarted`
- `SyncEvent::RemoteHostFinished`
- `SyncEvent::RemoteHostSkipped`
- `PricingStatus::SourceReported`

### function_missing

- `llmusage::commands::tui::run`

### inherent_method_missing

- `PricingCatalog::static_v1`

### method_parameter_count_changed

- `ReportCommonArgs::to_filter` 1 -> 2
- `SourceFileStore::counts` 1 -> 2
- `SourceFileStore::tracked_paths` 1 -> 2
- `SourceFileStore::sweep_missing` 2 -> 3
- `SourceFileStore::mark_inventory_seen` 3 -> 4
- `SourceFileStore::lossy_rebuild_risk` 1 -> 2
- `SyncStatusStore::load_source_sync_statuses` 0 -> 1
- `SyncStatusStore::save_source_sync_statuses` 1 -> 2
- `SyncStatusStore::mark_recent_completed` 2 -> 3
- `Store::reset_for_source` 1 -> 2
- `Store::mark_source_file_deleted` 2 -> 3
- `CursorStore::load_file_cursors` 1 -> 2
- `CursorStore::load_opencode_cursor` 0 -> 1
- `CursorStore::save_opencode_cursor` 1 -> 2
- `CursorStore::load_zcode_cursor` 0 -> 1
- `CursorStore::save_zcode_cursor` 1 -> 2

### module_missing

- `llmusage::commands::tui`

### struct_missing

- `llmusage::parsers::pi::PiParser`
- `llmusage::parsers::PiParser`

### struct_pub_field_missing

- `ReportFilter.since`
- `ReportFilter.until`
- `ReportFilter.timezone`
- `ReportFilter.source`

### warning partial_ord_enum_variants_reordered

- `SourceKind::Grok` position 7 -> 8
- `SourceKind::Zcode` position 8 -> 9
- `SourceKind::DeepseekHarness` position 9 -> 10
