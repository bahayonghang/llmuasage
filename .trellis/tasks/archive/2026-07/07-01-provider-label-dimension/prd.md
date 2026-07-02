# Native provider_label dimension for per-provider usage

## Goal

Add a first-class `provider_label` dimension to llmusage so usage (tokens/cost)
can be split by the **relay provider** a request actually went to (anyrouter /
methink / glm / …), not only by `source` (claude / codex / …). llmusage learns
the provider from an **external activation timeline** produced by CCR (the
config-switcher), consumed at sync time via an explicit `--provider-map` flag or
the default CCR activation log path when present.

## Background (verified against current code)

- Usage lives in SQLite (`~/.llmusage/llmusage.db`). Two tables carry it:
  - `usage_event` — one row per request (`src/store/migrations.rs:204-223`).
  - `usage_bucket_30m` — 30-minute rollups, PRIMARY KEY
    `(source, model, hour_start, project_hash)` (`src/store/migrations.rs:224-238`).
- Both are keyed only by **`source`** (`SourceKind`: Codex / Claude / Opencode /
  Antigravity — `src/domain/models.rs:7-20`). There is **no provider dimension**.
- The raw source logs llmusage parses (Claude/Codex JSONL etc.) do **not** contain
  the API `base_url` / provider, so llmusage cannot infer the provider on its own.
- CCR already records a **profile activation timeline** (append-only JSONL) every
  time the user switches profiles: `<CCR_ROOT>/analytics/provider_activation.jsonl`.
  That file is the authoritative `(source, time-window) → provider` mapping and is
  the input this task consumes.
- CCR resolves `<CCR_ROOT>` from `CCR_ROOT` or `~/.ccr`; the activation log path
  is `<root>/analytics/provider_activation.jsonl`. The CCR writer serializes
  `platform`, `profile`, `provider`, `provider_type`, `base_url_host`, `account`,
  `activated_at`, and `event`, and never writes auth tokens.
- Current schema version is **13** (`MIGRATIONS`, `latest_schema_version()` —
  `src/store/migrations.rs:45-74`). This task adds migration **v14**.

## Requirements

### Core (required — CCR reads these directly from the DB)
- FR1: Migration **v14** adds `provider_label` to **both** tables:
  - `usage_event.provider_label TEXT NOT NULL DEFAULT ''`.
  - `usage_bucket_30m.provider_label TEXT NOT NULL DEFAULT ''`, and it becomes
    part of the PRIMARY KEY. Empty string `''` = **unattributed** (see design §2
    for the SQLite NULL-in-PK rationale). Existing rows backfill to `''`.
- FR2: `UsageEvent` gains `provider_label` and it flows through event insertion
  **and** bucket rollup, so both tables stay consistent
  (`domain/models.rs`, `store/mod.rs` BucketKey, `store/sync_writer.rs`).
- FR3: Provider-map ingest builds per-`source` half-open time windows and stamps
  each event's `provider_label` by
  `(event.source == entry.platform AND event.event_at ∈ [t_i, t_{i+1}))`.
  No/`clear`/pre-first-window match → `''`. Deterministic and re-derivable under
  `sync --rebuild`. Timestamps are UTC RFC3339 on both sides.
- FR4: Coverage strategy is **B**: all sync entry points auto-load
  `${CCR_ROOT:-~/.ccr}/analytics/provider_activation.jsonl` when it exists and is
  readable; explicit `--provider-map <path>` overrides the default path. If no
  explicit path and no discovered readable file exist, sync behaves as today and
  `provider_label` stays `''`.
- FR5: Explicit `--provider-map <path>` is strict: missing/unreadable path errors
  the sync. Default auto-discovery is non-fatal: missing/unreadable default path
  leaves events unattributed and should only log at debug/warn level.

### Optional (only if llmusage's own dashboard/CLI should split by provider)
- FR6: `QueryFilter.provider: Option<String>` (`src/query/filter.rs`) and a
  `provider_breakdown()` mirroring `source_breakdown()`
  (`src/query/mod.rs:1061-1104`) + a `ProviderBreakdown` DTO.
- Note: CCR does **not** need FR6 — its adapter runs its own
  `GROUP BY provider_label` read-only SQL. FR6 is purely for llmusage's own UI.

## Provider-map input contract (produced by CCR — do not change unilaterally)

Append-only JSONL, one activation event per line, e.g.:
```json
{"platform":"claude","profile":"anyrouter3","provider":"anyrouter","provider_type":"official_relay","base_url_host":"anyrouter.top","account":"linuxdo_79797","activated_at":"2026-07-01T12:03:44+00:00","event":"activate"}
{"platform":"codex","profile":null,"provider":null,"activated_at":"2026-07-01T13:20:00+00:00","event":"clear"}
```
- Match key: `platform` ↔ llmusage `source` string (`claude`,`codex`,…).
- `provider` (nullable) is the value stamped into `provider_label`; `null`/`clear`
  → `''`.
- Interval construction: per `platform`, sort by `activated_at`; event *i* owns
  `[activated_at_i, activated_at_{i+1})`; last extends to `+∞`.
- Parser must be tolerant: skip malformed lines, ignore unknown platforms.

## Acceptance criteria

- [x] AC1: Migration v14 upgrades a seeded v13 DB: both tables gain
      `provider_label`; existing rows are `''`; `usage_bucket_30m` PK includes
      `provider_label`; `schema_version` = 14. (Migration unit test, seed→migrate→assert.)
- [x] AC2: Two events in the same (source, model, 30-min, project) window but
      different providers produce **two** bucket rows, not one.
- [x] AC3: With a sample `--provider-map`, events are stamped with the correct
      `provider_label`; events outside any window get `''`. (Ingest test.)
- [x] AC4: Without explicit `--provider-map` and without a discovered default map,
      usage aggregates and CLI summaries match pre-change behavior for the same
      inputs; the new schema column contains `''`.
- [x] AC5: Without explicit `--provider-map` but with the default CCR activation
      log present, normal sync attributes newly inserted events.
- [x] AC6: `sync --rebuild --provider-map <p>` and `sync --rebuild` with a default
      discovered map both re-derive labels deterministically.
- [x] AC7: `just ci` passes (`cargo fmt --check`, `cargo clippy --all-targets
      --all-features -D warnings`, `cargo test -- --test-threads=1`, docs build).

## Out of scope

- Inferring provider without CCR's timeline / from raw logs (impossible — logs
  lack base_url).
- Automatic reattribution of already-imported events during ordinary incremental
  sync. Historical labels require `sync --rebuild` with an explicit or discovered
  provider map; events predating the timeline stay `''`.
- Gemini/Antigravity & OpenCode provider attribution (CCR only feeds claude/codex
  for now; design must not preclude adding them).
- The CCR-side adapter/UI/TUI (separate CCR repo tasks).

## Constraints / conventions (this repo)

- Rust edition 2024; migrations are ordered in `MIGRATIONS` with inline
  seed→migrate→assert tests (pattern: `migration_v13_*` at
  `src/store/migrations.rs:1177-1305`).
- `AGENTS.md` directs writing/reading a `docs/adr/` ADR before schema changes —
  add an ADR for the provider dimension + the PK change.
- Preserve existing indexes on `usage_bucket_30m` across the table recreate.
- Record B's default CCR path coupling in the ADR so the llmusage/CCR dependency
  is explicit rather than hidden in CLI plumbing.
