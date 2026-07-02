# Design — Native provider_label dimension

> Technical design with real code anchors. Requirements/acceptance are in
> `prd.md`. The provider-map input shape is fixed by CCR (`prd.md` contract).

## §1 Touch-point map (verified anchors)

| Concern                                  | File                       | Anchor                        |
| ---------------------------------------- | -------------------------- | ----------------------------- |
| `usage_event` DDL                        | `src/store/migrations.rs`  | 204-223                       |
| `usage_bucket_30m` DDL (+PK)             | `src/store/migrations.rs`  | 224-238                       |
| MIGRATIONS array / latest ver            | `src/store/migrations.rs`  | 45-74 (current = 13)          |
| schema_version read/write                | `src/store/migrations.rs`  | 78-119                        |
| migration test pattern                   | `src/store/migrations.rs`  | 1177-1305 (`migration_v13_*`) |
| `UsageEvent` struct                      | `src/domain/models.rs`     | 94-113                        |
| `SourceKind` enum                        | `src/domain/models.rs`     | 7-20                          |
| in-memory `BucketKey`                    | `src/store/mod.rs`         | 612-617                       |
| `roll_up_bucket`                         | `src/store/sync_writer.rs` | 746-783                       |
| `flush_buckets_tx` (INSERT..ON CONFLICT) | `src/store/sync_writer.rs` | 820-906                       |
| `write_event_batch_tx` (event INSERT)    | `src/store/sync_writer.rs` | 288-402                       |
| reset replay bucket rollback             | `src/store/sync_writer.rs` | 122-277                       |
| reset pricing refresh                    | `src/store/sync_writer.rs` | 908-953                       |
| cost recompute bucket reconciliation      | `src/store/mod.rs`         | 301-435                       |
| `Store::begin_sync_run` / writer fields   | `src/store/sync_writer.rs` | 56-72, 466-485                |
| `SyncRunOptions`                         | `src/commands/sync.rs`     | 28-36                         |
| sync CLI (clap) → options                | `src/commands/mod.rs`      | 64-78, 238-244                |
| background job sync options               | `src/sync/job_registry.rs` | 286                           |
| `QueryFilter`                            | `src/query/filter.rs`      | 23-42                         |
| `source_breakdown` + DTO                 | `src/query/mod.rs`         | 1061-1104, 185-196            |

## §2 Schema — migration v14

Append to `MIGRATIONS` (`migrations.rs:45-67`): `(14, "add_provider_label",
m_014_add_provider_label)`; `latest_schema_version()` becomes 14.

### 2.1 `usage_event` — simple ADD COLUMN (event_key is PK, unchanged)

```sql
ALTER TABLE usage_event ADD COLUMN provider_label TEXT NOT NULL DEFAULT '';
```

### 2.2 `usage_bucket_30m` — PK changes → must recreate the table

SQLite cannot alter a PRIMARY KEY in place, so recreate + copy + drop + rename.
**Verified v13 column set** (baseline @224-238; v2 renames `cached_input_tokens`→
`cache_read_tokens` and adds `cache_creation_tokens`; v3 adds 5 cost/pricing cols
to _both_ tables @351-372; v4 adds `event_count` @380). Author the new table
against **exactly** this set — same columns, nothing added or dropped:

```sql
CREATE TABLE usage_bucket_30m__v14 (
    source                  TEXT    NOT NULL,
    provider_label          TEXT    NOT NULL DEFAULT '',               -- NEW (v14)
    model                   TEXT    NOT NULL,
    hour_start              TEXT    NOT NULL,
    project_hash            TEXT    NOT NULL DEFAULT '',
    project_label           TEXT,
    project_ref             TEXT,
    input_tokens            INTEGER NOT NULL,
    cache_read_tokens       INTEGER NOT NULL,                          -- v2 rename of cached_input_tokens (no DEFAULT)
    output_tokens           INTEGER NOT NULL,
    reasoning_output_tokens INTEGER NOT NULL,
    total_tokens            INTEGER NOT NULL,
    updated_at              TEXT    NOT NULL,
    cache_creation_tokens   INTEGER NOT NULL DEFAULT 0,                -- v2
    cost_with_cache_usd     REAL    NOT NULL DEFAULT 0.0,              -- v3
    cost_without_cache_usd  REAL    NOT NULL DEFAULT 0.0,              -- v3
    pricing_status          TEXT    NOT NULL DEFAULT '<PRICING_UNPRICED>', -- v3: substitute the real const value
    pricing_source          TEXT,                                     -- v3
    pricing_rate            TEXT,                                     -- v3: TEXT, NOT REAL
    event_count             INTEGER NOT NULL DEFAULT 0,                -- v4
    PRIMARY KEY (source, provider_label, model, hour_start, project_hash)
);
INSERT INTO usage_bucket_30m__v14 (
    source, provider_label, model, hour_start, project_hash, project_label, project_ref,
    input_tokens, cache_read_tokens, output_tokens, reasoning_output_tokens, total_tokens,
    updated_at, cache_creation_tokens, cost_with_cache_usd, cost_without_cache_usd,
    pricing_status, pricing_source, pricing_rate, event_count
)
SELECT
    source, '', model, hour_start, project_hash, project_label, project_ref,
    input_tokens, cache_read_tokens, output_tokens, reasoning_output_tokens, total_tokens,
    updated_at, cache_creation_tokens, cost_with_cache_usd, cost_without_cache_usd,
    pricing_status, pricing_source, pricing_rate, event_count
FROM usage_bucket_30m;
DROP TABLE usage_bucket_30m;
ALTER TABLE usage_bucket_30m__v14 RENAME TO usage_bucket_30m;
-- Recreate the ONE existing index (verified @migrations.rs:297-298):
CREATE INDEX IF NOT EXISTS idx_usage_bucket_30m_hour_start ON usage_bucket_30m(hour_start);
```

> **Corrected vs. the earlier draft:** (1) there is no separate `cached_input_tokens`
> column — v2 renamed it to `cache_read_tokens`; (2) `pricing_rate` is `TEXT`, not
> `REAL`. Seed the migration unit test by running migrations **1..13** (not a
> hand-written v13 DDL) so any column drift fails loudly. Alternative: build the
> copy dynamically via `PRAGMA table_info(usage_bucket_30m)` to be immune to drift.

### 2.3 Why `NOT NULL DEFAULT ''` and not nullable (critical SQLite gotcha)

In SQLite a `PRIMARY KEY` column **may hold NULL, and NULLs compare as distinct** —
so `ON CONFLICT` would never dedup two NULL-provider buckets, silently exploding
rows. Using `NOT NULL DEFAULT ''` with `''` = unattributed keeps the existing
upsert semantics intact. This mirrors how `project_hash` is already
`NOT NULL DEFAULT ''` in this table.

## §3 Event & bucket plumbing

1. `UsageEvent` (`domain/models.rs:94-113`): add
   `pub provider_label: String` (default `""`). Update all constructors; parsers
   set `""` (they don't know provider — it's stamped later, §4).
2. `BucketKey` (`store/mod.rs:612-617`): add `provider_label: String` so buckets
   group per provider. **Two** constructors build a `BucketKey` and both must set
   the field (the compiler flags the missing one): `roll_up_bucket`
   (`sync_writer.rs:756`) and a second site at `store/mod.rs:392` — confirm what
   the `:392` path does and feed it the same label semantics.
3. `roll_up_bucket` (`sync_writer.rs:746-783`): set
   `provider_label: event.provider_label.clone()` in the key.
4. `flush_buckets_tx` (`sync_writer.rs:820-906`): add `provider_label` to the
   INSERT column list, the `VALUES`, and the `ON CONFLICT(...)` target
   `(source, provider_label, model, hour_start, project_hash)`.
5. `write_event_batch_tx` (`sync_writer.rs:288-402`): add `provider_label` to the
   `usage_event` INSERT.

### 3.1 Old bucket-key SQL that must change with the PK

Changing the bucket PK is not limited to the insert path. Every SQL path that
currently treats `(source, model, hour_start, project_hash)` as the full bucket
identity must include `provider_label`:

- `reset_file_events_batch_tx` (`sync_writer.rs:122-277`): aggregate old
  `usage_event` rows with `provider_label`; update/delete the matching
  `usage_bucket_30m` row with `provider_label`; carry touched buckets as
  `(provider_label, model, hour_start, project_hash)` so pricing refresh does not
  merge providers during replay.
- `refresh_bucket_pricing_after_reset_tx` (`sync_writer.rs:908-953`): select
  matching events and update bucket pricing by provider as well as source/model
  / hour/project.
- `Store::recompute_costs_with` (`store/mod.rs:301-435`): select
  `provider_label` from `usage_event`, add it to `PricingRecomputeRow` and
  `BucketKey`, update bucket costs by the new full PK, and include provider in
  the delete-empty `NOT EXISTS` reconciliation.
- Direct test/helper SQL with old `ON CONFLICT(source, model, hour_start,
  project_hash)` must be updated. Current hits include `src/testing/mod.rs`,
  `src/web/mod.rs`, `tests/report_commands.rs`, and query/report test fixtures
  under `src/query/`.

## §4 Ingest — `--provider-map`

### 4.1 CLI + options

- `SyncRunOptions` (`sync.rs:28-36`) derives `Default`; add
  `pub provider_map: Option<PathBuf>` (defaults to `None`). `None` means
  "auto-discover the CCR default path", not "disable provider attribution".
- Sync clap variant (`commands/mod.rs:64-78`): add
  `#[arg(long, value_name = "PATH")] provider_map: Option<PathBuf>`; thread it
  into the `SyncRunOptions { … }` construction at `commands/mod.rs:238-244`.
- **Full-literal sites** (all fields named → won't compile without the new field):
  `commands/mod.rs:238` (pass the CLI arg), `hook_run.rs:61`,
  `src/sync/job_registry.rs:286` (set `provider_map: None` so they use default
  discovery).
  `tui/mod.rs:191` uses `..SyncRunOptions::default()` — no change needed.

> **Coverage decision: B is accepted.** All sync entry points use the same
> resolver: explicit `--provider-map` overrides; otherwise resolve
> `${CCR_ROOT:-~/.ccr}/analytics/provider_activation.jsonl` and load it when
> present/readable. This is necessary because most imports happen via hook,
> background job, TUI, or library sync. Since events are inserted once with
> `INSERT OR IGNORE` (§4.4), CLI-only attribution would leave rows imported by
> other paths permanently `''` until a rebuild.
>
> Error behavior: explicit path missing/unreadable is a sync error. Default
> discovery missing/unreadable is non-fatal and leaves events unattributed; log
> at debug/warn level so machines without CCR keep syncing.

### 4.2 Provider index (new small module, e.g. `src/domain/provider_map.rs`)

```rust
// Parse CCR activation JSONL → per-source sorted (activated_at, provider_label).
// Empty provider / "clear" / no-match → "".
pub struct ProviderIndex { /* HashMap<SourceKind, Vec<(String /*rfc3339*/, String)>> */ }
impl ProviderIndex {
    pub fn load(path: &Path) -> Result<Self>;               // tolerant: skip bad lines
    pub fn label_for(&self, source: SourceKind, event_at: &str) -> String; // binary search
}
```

- `label_for`: within the source's sorted vec, find the last entry with
  `activated_at <= event_at`; return its provider (or `""`). Compare RFC3339
  strings via parsed `DateTime`, not lexically (offsets differ: `+00:00` vs `Z`).
- Keep this module below `store`/`commands` dependencies. `SyncRunWriter` lives
  in `store`, so the writer should depend on a domain-level `ProviderIndex`, not
  on `crate::sync::*`.

### 4.3 Stamp point (single, verified seam)

Resolve/load the provider index once in `run_once_locked` after any rebuild reset
and before parser work starts. Add writer plumbing rather than hiding it in the
store constructor:

- Keep `Store::begin_sync_run()` as the no-provider convenience wrapper for tests
  and existing callers.
- Add `Store::begin_sync_run_with_provider_index(provider_index:
  Option<ProviderIndex>) -> Result<SyncRunWriter>`.
- Add `provider_index: Option<ProviderIndex>` to `SyncRunWriter`.

The **verified single stamp seam** is `commit_shard_inner`
(`sync_writer.rs:500`, signature `(&mut self, shard: SyncShard, …)` — `shard` is
owned by value). Make it `mut shard`, and immediately before the write loop
`for batch in shard.events.chunks(EVENT_WRITE_BATCH_SIZE)` (`:541`) stamp every
event in one owned pass:

```rust
if let Some(index) = self.provider_index.as_ref() {
    for ev in &mut shard.events {
        ev.provider_label = index.label_for(ev.source, &ev.event_at);
    }
}
```

Every source funnels through `commit_shard_inner`, so this one point covers all.
Both the event INSERT (`write_event_batch_tx`) and `roll_up_bucket` then read the
same `event.provider_label`, keeping the two tables consistent.

> **Do not** stamp inside `write_event_batch_tx`: it receives
> `events: &[UsageEvent]` (immutable) via `shard.events.chunks()`. Stamp the owned
> `shard.events` _before_ chunking — that is the clean, mutable seam.

### 4.4 Rebuild semantics (AC6) — must replay from empty

`write_event_batch_tx` uses `INSERT OR IGNORE` and only calls `roll_up_bucket` for
**newly inserted** events (`sync_writer.rs:384-392`). So ordinary re-sync does not
re-stamp events already in `usage_event`, and their buckets are not recomputed.

Verified rebuild flow: `run_once_locked` calls `reset_for_rebuild`; that calls
`Store::reset_for_source` or `Store::reset_usage_data`, and those delete
`usage_event` plus `usage_bucket_30m` before replay (`src/store/schema.rs:104-152`).
Therefore AC6 is covered when the stamp happens before reinsert. Add a regression
test for rebuild with both explicit and default-discovered provider maps.

## §5 Optional query mirror (FR6 — llmusage UI only)

- `QueryFilter` (`filter.rs:23-42`): add `pub provider: Option<String>` only if
  llmusage's own UI/API needs it. Do **not** add provider filtering to the generic
  `sql_filter_with_model_column`: that helper also feeds `turn_filter` and
  `tool_filter`, but `usage_turn` / `usage_tool_call` do not have
  `provider_label`.
- If FR6 is implemented, apply provider filters only to `bucket_filter` and
  `event_filter`, or join behavior/tool queries through `usage_event` when a
  provider filter is requested.
- `provider_breakdown()` (new, next to `source_breakdown` at `query/mod.rs:1061`):
  same shape, `GROUP BY provider_label`; `ProviderBreakdown { provider_label,
total_tokens, last_event_at, event_count }`.

## §6 Compatibility, rollback, ADR

- Additive + defaulted: old code paths keep working. With no explicit map and no
  discovered readable default map, provider stays `''` and usage aggregates are
  unchanged. With the default CCR map present, attribution starts automatically
  in every sync entry point.
- Downgrade note: v14 recreates `usage_bucket_30m`; a DB opened by an older
  binary (schema-version guard) should refuse gracefully — follow the existing
  version-gate behavior; document that downgrade requires rebuild.
- Historical attribution note: already-imported events are not restamped by
  ordinary incremental sync because of `INSERT OR IGNORE`; users must run
  `sync --rebuild` with an explicit or discovered map to rederive labels.
- Rollback: revert the migration entry + plumbing; a v14 DB can be rebuilt from
  source logs (`sync --rebuild`). No data loss if the raw logs still exist; the
  existing lossy-rebuild guard still applies when source files are missing.
- Write a `docs/adr/` entry (per `AGENTS.md`) recording: the provider dimension,
  the `''`-sentinel + PK change, and the B default-map ingest decision.

## §7 Test plan (mirror existing patterns)

- Migration unit test (in `migrations.rs` tests, like `migration_v13_*`): seed a
  v13 DB with bucket+event rows → run v14 → assert columns added, rows `''`, new
  PK present, `schema_version == 14`.
- `ProviderIndex` unit tests: interval boundaries (inclusive start / exclusive
  end), clear→`""`, pre-first-window→`""`, offset-format equivalence
  (`+00:00` vs `Z`), malformed-line tolerance.
- Ingest/integration test (under `tests/`): tiny fixture source logs + a
  provider-map → assert `usage_event.provider_label` and that same-window
  different-provider events yield distinct buckets (AC2).
- Regression: no explicit map + no discovered default map → identical aggregates
  to pre-change and `provider_label=''`.
- Default-map regression: no explicit map + default CCR activation log present →
  newly inserted events are stamped.
- Rebuild regression: `sync --rebuild` re-derives labels with both explicit and
  default-discovered provider maps.
- Gate: `just ci`.
