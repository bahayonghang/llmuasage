# Changelog

## 0.5.0 - 2026-05-08

### Added

- Added an explicit SQLite migration runner with automatic `llmusage.db.pre-0.5.0` backups for v0 databases. The 0.5.0 schema advances to v10 with real migrations for cache split, cost metadata, project/event counts, `source_file`, recent/history sync status, raw archive, worker lock metadata, Gemini registration, and `pricing_catalog_version`.
- Added `--home <PATH>`, `LLMUSAGE_HOME`, `AppPaths::with_root`, and `AppPaths::with_cli_home` so CLI and embedded adapters can run against isolated local roots.
- Added Gemini support: `SourceKind::Gemini`, a Gemini parser for local `~/.gemini/tmp/*/chats/session-*.json` files, and Gemini `SessionEnd` hook installation/probe/uninstall.
- Added the ccr-ui read surface: `Dashboard::overview`, `home_overview`, `heatmap`, `logs`, archive diagnostics, source-file forget, and HTTP routes for the same local data.
- Added in-process import jobs through `JobRegistry`, including progress snapshots, cancellation, recent/history completion metadata, and `sync --json-events` support.
- Added local pricing catalogs: embedded `pricing/static-v1.json`, local snapshot loading, `doctor --refresh-pricing <file>`, and persisted `meta('pricing_catalog_version')`.
- Added `LlmusageError` as the public non-exhaustive error surface for downstream adapters.
- Added `testing::Fixture` behind the `testing` feature for downstream integration tests.
- Added ADRs 0004-0007 covering migrations, job registry, source-file state, and error surface.

### Changed

- Switched report command JSON (`daily`, `monthly`, `session`, `blocks`) and web/export derived keys to snake_case. See the JSON naming migration below.
- Reworked sync writes around `SyncShard` and `commit_shard`, preserving incremental cursors while reducing full-source buffering.
- Renamed the worker table from `worker_lease` to `worker_lock`; CLI/library sync now waits on a holder-aware lock while hook-run remains non-blocking.
- Report commands remain read-only and no longer imply sync. Run `llmusage sync` or `llmusage sync --rebuild` when local source data or upgrade-derived metadata needs refreshing.

### Local-only boundary

- No upload queue, no login, no device token, and no remote pricing fetch were added.
- `doctor --refresh-pricing <file>` reads only a local JSON file and writes local SQLite cost metadata.
- `diagnostics --forget-file` only mutates local source-file/cursor state.

### Migration notes

- Existing v0/v1 databases are backed up before migration and then upgraded to schema v10.
- JSON consumers should update camelCase report-field lookups to snake_case.
- If session/source-file/archive metadata is missing after upgrade, run `llmusage sync --rebuild` to repopulate it from local sources.

### 0.5.0 JSON naming migration- Switched report command JSON (`daily`, `monthly`, `session`, `blocks`) from camelCase to snake_case to match HTTP API, static export snapshots, and SQLite field names.
- 0.4.x → 0.5.0 field map for jq/scripts:
  - `totalTokens` → `total_tokens`
  - `inputTokens` → `input_tokens`
  - `cacheReadTokens` / `cachedInputTokens` → `cache_read_tokens`
  - `cacheCreationTokens` → `cache_creation_tokens`
  - `outputTokens` → `output_tokens`
  - `reasoningOutputTokens` → `reasoning_output_tokens`
  - `estimatedCostUsd` → `estimated_cost_usd`
  - `projectHash` / `projectLabel` / `projectRef` → `project_hash` / `project_label` / `project_ref`
  - `sessionId` / `sessionLabel` → `session_id` / `session_label`
  - `firstActivityAt` / `lastActivityAt` → `first_activity_at` / `last_activity_at`
  - `blockId` / `startAt` / `endAt` / `isActive` → `block_id` / `start_at` / `end_at` / `is_active`
  - `durationMinutes` / `burnRateTokensPerHour` / `projectedTotalTokens` → `duration_minutes` / `burn_rate_tokens_per_hour` / `projected_total_tokens`
  - `tokenLimit` / `tokenLimitPercent` → `token_limit` / `token_limit_percent`
  - `modelsUsed` / `modelBreakdowns` → `models_used` / `model_breakdowns`
