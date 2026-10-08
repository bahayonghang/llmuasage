# Implementation plan — dynamic pricing on sync

Do not start until this plan is reviewed.

## 1. Price refresh

- Add a sync-owned refresh for the LiteLLM and models.dev URLs.
- Skip a source whose successful cache is no older than 3600 seconds.
- Fetch a source only when its cache is missing, older than 3600 seconds, or has a future timestamp.
- Inject the HTTP client so tests use a local fixture.
- Save each successful body with its timestamp, using temp-file rename.
- On failure, keep the previous cache at any age and continue.
- Fall back to the embedded catalog only when no source cache exists.

## 2. Catalog import

- Convert the fetched tables into the existing catalog document, including long-context tiers.
- Let exact fetched model rows beat broader embedded families.
- Recompute catalog-priced events when the selected base changes.
- Leave positive source-reported costs unchanged.
- Re-apply an existing user overlay after the base is selected.

## 3. Sync integration

- Call the refresh once from `run_once_locked_with_remote_source` before parser work.
- Keep fallback warnings off `sync --json-events` stdout.

## 4. Version and docs

- Move user-facing versions and the unreleased changelog heading to `2.0.1`.
- Replace the README and docs statements that say llmusage never fetches pricing.

## Validation

- Fixture tests for a fresh-cache skip, a stale-cache fetch, per-source failure, cold start, tier selection, overlay precedence, and JSON stdout.
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- Targeted pricing, catalog, and sync tests, then the Rust gate required by the diff.
- `npm --prefix docs run docs:build` if docs change.
