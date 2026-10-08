# Design — sync 时刷新公开价表

## Classification

Complex. Adds a network read to default sync, a local price cache, tier-preserving import, and a 2.0.1 version bump. It does not add a source, parser, or SQLite schema migration.

## Refresh point

`src/sync/engine.rs` `run_once_locked_with_remote_source` is the one-run entry used by the CLI and the dashboard job. Perform the price refresh there once, after option validation and before `registered_parsers()` is driven.

Event cost is already calculated at local shard commit from the active catalog (`src/store/sync_writer.rs`). Remote shards therefore use the operator machine's refreshed catalog. No usage payload is sent.

## Sources

Each source is decided on its own, before any log parser runs. A successful cache whose age is at most 3600 seconds is used as-is and is not requested. Only a missing cache, or one older than 3600 seconds, produces an HTTPS GET. The two possible requests, and nothing else, are:

- `https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json`
- `https://models.dev/api.json`

LiteLLM is the base table. models.dev fills models and long-context rates that the base table does not publish. OpenRouter is not requested.

Use a bounded timeout. Tests inject a local fixture client. Production code must not be invoked against the live URLs by the test suite.

## Cache

Store the last successful body for each source under the local llmusage runtime root, with the time it was saved. Write with a temp file and rename, so a failed write cannot destroy the previous cache.

Age is `now - saved_at` in seconds. At most 3600 is fresh and skips the network, matching tokscale `CACHE_TTL_SECS`. A timestamp in the future is not a usable fresh cache, so that source is fetched. An error, timeout, empty body, or document with no usable price rows leaves that source's cache untouched and falls back to it at any age. A warning goes to the existing human sync channel or stderr. It must not be written to `sync --json-events` stdout. Skipping a fresh cache is not a failure and does not warn.

Selection, per source:

1. Successful cache no older than 3600 seconds.
2. Otherwise a usable response from this run.
3. Otherwise that source's previous successful cache, at any age.
4. If both sources have neither, the embedded `static-v3` catalog.

A response that replaces a source cache replaces the sync-managed base and recomputes catalog-priced events, including events whose model was already known. A skipped fresh cache, or a response whose content is unchanged, does not recompute. `source_reported` rows with a positive total stay unchanged.

## Catalog shape

Map published long-context fields into existing catalog tiers. Do not reuse the current native LiteLLM importer path that sets `tiers` to an empty list.

Exact model identity wins over a broader family. A fetched `claude-sonnet-5-5` must not be priced by family `claude-sonnet`. The same rule applies to `claude-haiku-5-5` and `gpt-6.1-sol`.

`catalog apply` still replaces complete model definitions by id after the refreshed base is assembled.

## What changes in the old pin rule

The next default sync no longer leaves prices frozen at the embedded catalog or at the last `doctor --refresh-pricing` snapshot. The sync-managed cache becomes the base. The user overlay remains the only persistent manual override.

## Version

Bump user-facing metadata from `2.0.0` to `2.0.1`: root `Cargo.toml`, desktop crate, `desktop/src-tauri/tauri.conf.json`, `desktop/package.json`, `desktop/package-lock.json`, and both READMEs. Replace the unreleased `2.0.0` changelog heading with `2.0.1`. Update the README sentences that say pricing is never fetched.
