# Implement — Native provider_label dimension

Ordered plan. Anchors and rationale live in `design.md`; requirements in `prd.md`.
Complex task → this file + `design.md` must exist before `task.py start`.

## Stage 0 — ADR + fixtures

1. [x] Write a `docs/adr/` entry (per `AGENTS.md`): provider dimension, `''`
       sentinel, `usage_bucket_30m` PK change, and B's default CCR map discovery
       (`${CCR_ROOT:-~/.ccr}/analytics/provider_activation.jsonl`) with explicit
       `--provider-map` override.
2. [x] Add a sanitized CCR activation JSONL fixture from the `prd.md` contract
       (synthetic is fine; real local samples must not leak account secrets).

## Stage 1 — Migration v14 (schema)

3. [x] `src/store/migrations.rs`: append `(14, "add_provider_label",
 m_014_add_provider_label)` to `MIGRATIONS` (45-67); bump
       `latest_schema_version()` → 14.
4. [x] Implement `m_014_*`: `ALTER TABLE usage_event ADD COLUMN provider_label
 TEXT NOT NULL DEFAULT ''`; recreate `usage_bucket_30m` with
       `provider_label TEXT NOT NULL DEFAULT ''` in the PK (design §2.2) — copy
       rows with `provider_label=''`, drop/rename, **recreate existing indexes**.
5. [x] Migration unit test (seed v13 → migrate → assert), mirroring
       `migration_v13_*` (`migrations.rs:1177-1305`).

- Validate: `cargo test store::migrations -- --test-threads=1`.

## Stage 2 — Event & bucket plumbing

6. [x] `src/domain/models.rs:94-113`: add `UsageEvent.provider_label: String`
       (default `""`); set `""` at every constructor — parsers `claude.rs`,
       `codex.rs`, `opencode.rs` plus test builders (the compiler flags each).
7. [x] `src/store/mod.rs:612-617`: add `provider_label` to `BucketKey`; update
       **both** constructors — `roll_up_bucket` (`sync_writer.rs:756`) and
       `store/mod.rs:392`.
8. [x] `src/store/sync_writer.rs`: set key in `roll_up_bucket` (746-783); add the
       column to `flush_buckets_tx` INSERT/VALUES/ON CONFLICT (820-906) and to
       `write_event_batch_tx` event INSERT (288-402).
9. [x] Update every old bucket-key SQL path to include provider:
       `reset_file_events_batch_tx` aggregate/update/delete/touched-buckets,
       `refresh_bucket_pricing_after_reset_tx`, and `Store::recompute_costs_with`
       SELECT/row struct/BucketKey/update/delete-empty reconciliation.
10. [x] Update direct seed/helper SQL that inserts/upserts `usage_bucket_30m` with
        the old conflict key. Current grep hits include `src/testing/mod.rs`,
        `src/web/mod.rs`, `tests/report_commands.rs`, and query/report fixtures
        under `src/query/`.

- Validate: `cargo build`; existing tests still green.

## Stage 3 — Ingest (`--provider-map`)

11. [x] `SyncRunOptions.provider_map: Option<PathBuf>` (`sync.rs:28-36`, derives
        Default); add the clap `--provider-map` arg + thread into options
        (`commands/mod.rs:64-78`, 238-244); add `provider_map:` to the full-literal
        sites `hook_run.rs:61` and `src/sync/job_registry.rs:286`
        (`tui/mod.rs:191` uses `..default()`, skip). `None` means default
        auto-discovery, not disabled attribution.
12. [x] New `src/domain/provider_map.rs`: `ProviderIndex::{load, label_for}`
        (design §4.2) — tolerant parse, per-source sorted windows, RFC3339 compare
        via parsed `DateTime` (handle `+00:00` vs `Z`). Add a resolver for
        explicit path vs `${CCR_ROOT:-~/.ccr}/analytics/provider_activation.jsonl`:
        explicit missing/unreadable = error; default missing/unreadable = no
        index + debug/warn log.
13. [x] Writer plumbing: keep `Store::begin_sync_run()` as a no-provider wrapper;
        add `Store::begin_sync_run_with_provider_index(Option<ProviderIndex>)`;
        add `provider_index` to `SyncRunWriter`; in `run_once_locked`, resolve/load
        the provider index once before creating the writer.
14. [x] Stamp at the verified seam: in `commit_shard_inner` (`sync_writer.rs:500`,
        make `shard` `mut`) loop `&mut shard.events` setting `provider_label`
        before the `chunks()` write loop (`:541`), when `provider_index` is `Some`.
        Not inside `write_event_batch_tx` (immutable slice). See design §4.3.
15. [x] Unit tests for `ProviderIndex` (boundaries, clear→`""`, pre-window→`""`,
        offset formats, malformed lines) + an ingest integration test under
        `tests/` (AC2: same-window different-provider → distinct buckets; AC5:
        default CCR map stamps events without an explicit flag).

- Validate: `cargo test -- --test-threads=1`.

## Stage 4 — Optional query mirror (FR6, only if llmusage UI needs it)

16. [x] Skipped for CCR MVP. If implemented, add `QueryFilter.provider`
        (`filter.rs:23-42`) only to bucket/event filters or behavior queries that
        explicitly join `usage_event`; do not add it to the generic SQL helper used
        by `turn_filter`/`tool_filter`. Add `provider_breakdown()` +
        `ProviderBreakdown` next to `source_breakdown` (`query/mod.rs:1061`).

- Skippable for the CCR integration (CCR reads the DB directly).

## Stage 5 — Gate

17. [x] `rg "ON CONFLICT\\(source, model, hour_start, project_hash\\)|WHERE source = \\?1 AND model = \\?2 AND hour_start = \\?3 AND project_hash = \\?4|GROUP BY model, hour_start, COALESCE\\(project_hash"` returns no unreviewed old-key bucket SQL.
18. [x] `just ci` (`cargo fmt --check`, `clippy --all-targets --all-features -D
warnings`, `cargo test -- --test-threads=1`, docs build).

## Regression guards

- No explicit `--provider-map` + no discovered default map → aggregates identical
  to pre-change and all labels `''` (AC4).
- No explicit `--provider-map` + default CCR map present → newly inserted events
  are attributed (AC5).
- `sync --rebuild --provider-map <p>` and `sync --rebuild` with the default map
  re-derive labels deterministically (AC6). Existing code already wipes
  `usage_event` + `usage_bucket_30m` before replay; keep the regression so
  `INSERT OR IGNORE` cannot hide stale labels later.

## Rollback

Revert the migration entry + plumbing; a v14 DB rebuilds from source logs via
`sync --rebuild`. Raw logs are the source of truth — no data loss.

## Cross-repo note

Input format is owned by CCR (task `07-01-provider-activation-timeline`, already
implemented). CCR's adapter/sync (`07-01-llmusage-provider-ingest-adapter`) gates
on `schema_version >= 14`; it can rely on default discovery when sharing the same
`CCR_ROOT`, or pass `--provider-map` explicitly for alternate roots/tests. Keep
the JSONL shape and the schema version in sync across both repos.
