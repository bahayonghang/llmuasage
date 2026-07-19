# llmusage Pricing Catalog Contracts

## Scenario: Versioned Model Pricing And Context Windows

### 1. Scope / Trigger

- Trigger: changes to `pricing/static-v2.json`, `PricingCatalog`, catalog activation,
  cost calculation, model pricing fields, or context-pressure windows.
- Pricing is cross-layer: parser model strings flow into SQLite events, catalog
  lookup, persisted costs, 30-minute buckets, reports, dashboards, and context
  pressure.
- Parser-owned model names remain raw identifiers. Catalog recognition must not
  rename stored events or merge model buckets.

### 2. Catalog V2 Signatures

- Embedded base: `pricing/static-v2.json`.
- Top level: `schema_version = 2`, `kind = base|overlay`, human `version`,
  `models`, and overlay-only `remove_models`.
- Model: stable `id`, one or more `sources`, explicit `matches`, `rates.default`,
  optional `rates.tiers`, and optional positive `context_window`.
- Matcher modes: `exact` or `family`. Exact wins over family; within one mode,
  the longest normalized matcher wins.
- Runtime APIs: `PricingCatalog::embedded()`, `load_snapshot`, `find`,
  `context_window`, and Store catalog apply/status/reset services.
- `PricingCatalog::static_v1()` is only a deprecated source-compatibility wrapper
  around the current embedded catalog.

### 3. Model And Matcher Contracts

- A model definition is keyed by stable `id`; `sources` expands it into
  source-specific runtime rules without duplicating rates.
- `exact` matches only the complete normalized model id. `family` also accepts a
  dash-delimited normalized suffix. Normalization lowercases, strips provider
  prefixes, and maps dot/underscore/colon separators to dash.
- Empty/duplicate model ids, sources, matchers, and ambiguous compiled matchers
  are rejected before activation.
- Aliases belong in catalog data. Do not add production Rust alias branches for
  ordinary model additions.
- Unknown models remain `unpriced` and have no context denominator. Do not infer
  a rate or context window from a broad provider default.

### 4. Rate And Tier Contracts

- Every rate channel must be finite and non-negative. Cache creation defaults to
  uncached input only when the source document omits it.
- Tier names are unique and `prompt_tokens_above` thresholds are positive and
  strictly increasing.
- Tier selection is per `usage_event`:

  ```text
  prompt_tokens = input + cache_read + cache_creation
  selected = highest tier where prompt_tokens > prompt_tokens_above
  ```

- Buckets and reports sum persisted event costs. They must never reapply a tier
  threshold to aggregate tokens.
- `pricing_rate` records the stable model id, selected tier, prompt-token count,
  threshold, actual channel rates, and reasoning policy.

### 5. Overlay And Activation Contracts

- Overlay merge order is: validate base, strictly remove ids, completely replace
  or append definitions by id, then validate the full effective catalog.
- Replacement is whole-model. Never deep-merge rate or matcher fields.
- A second apply uses the recorded base, not the previous effective catalog.
- Persist canonical files as `base-<sha256>.json`,
  `overlays/overlay-<sha256>.json`, and `effective-<sha256>.json`. The declared
  `version` is an audit label and never participates in path construction.
- Write and validate files first, recompute events page-by-page, reconcile bucket
  pricing, then switch active/base/overlay meta in the final transaction.
- `Store::recompute_costs()` recalculates with the selected catalog without
  clearing its base/overlay metadata. Repricing must not change layer ownership.
- `Store::recompute_costs_with(custom)` persists non-embedded catalog content
  under a digest filename before switching metadata, including when the caller
  labels the catalog `PricingStatus::Static`.
- Once meta selects a user file, missing content, digest mismatch, parse failure,
  or metadata/document mismatch fails closed. Do not fall back to embedded data.
- `catalog reset` restores a pinned snapshot base. An embedded base returns to
  the current embedded catalog. Reset without an overlay is idempotent.
- `doctor --refresh-pricing` activates a complete internal-v1, v2, or native
  LiteLLM base snapshot and clears any overlay; it is not an overlay path.
- Old `pricing/<version>.json` snapshots remain readable when only legacy
  `pricing_catalog_version` meta exists.

### 6. Embedded Upgrade Contract

- Bootstrap upgrades an unpinned old `static-*` catalog to the current embedded
  catalog and recomputes costs.
- Complete snapshots remain pinned.
- Overlays based on an old embedded catalog remain pinned; status reports
  `rebase_available` until the user applies again or resets.
- Event repricing remains paged and each committed page may emit observational
  progress. Bucket updates, orphan deletion, and activation metadata remain one
  final transaction; failure before that commit must not emit completion or
  advance the active catalog.
- Final bucket reconciliation must reuse the recomputed in-memory bucket-key
  map. It reads persisted bucket primary keys once, updates matching keys, and
  deletes only persisted keys absent from the map. Reconciliation must not read
  `usage_event`, add a permanent index, or introduce a schema migration.
- Bootstrap pricing progress is ordered as started, throttled committed-page
  progress, bucket-reconcile started, and finished. Emit progress at most once
  per second or every 25,000 events, plus the final page. Current and pinned
  catalogs emit no pricing lifecycle.
- Structured pricing logs use stable operation/phase/version/count/elapsed
  fields without paths, event keys, prompts, or catalog contents. Phase
  boundaries are `info`, page progress is `debug`, continued work past 30
  seconds emits exactly one `warn`, and failures are `error`.

### 7. GPT-5.6 Contract

- `gpt-5.6-luna`, `gpt-5.6-terra`, and `gpt-5.6-sol` are exact entries for
  `codex` and `opencode`, each with context window `1_050_000`.
- Exact alias `gpt-5.6` resolves to Sol. It must not claim
  `not-gpt-5.6-*` or preview suffixes.
- Default input/cache-write/cache-read/output MTok rates:
  - Luna: `1.0 / 1.25 / 0.1 / 6.0`
  - Terra: `2.5 / 3.125 / 0.25 / 15.0`
  - Sol: `5.0 / 6.25 / 0.5 / 30.0`
- Above 272,000 prompt tokens, all input channels use 2x default and output uses
  1.5x default. Exactly 272,000 remains on the default tier.

### 8. Tests Required

- Embedded load, positive source/model/alias matches, and negative non-overlap.
- Invalid schema, identifiers, matcher duplication, rates, thresholds, and
  strict unknown removal.
- Internal-v1 and native LiteLLM compatibility.
- Short/272K/long exact costs, cache creation, audit JSON, and mixed-tier bucket.
- Overlay add/replace/remove, repeated apply, process restart, reset, digest
  corruption, old-path compatibility, embedded upgrade, and snapshot pinning.
- Active-catalog recompute preserves overlay metadata; direct custom-catalog
  recompute survives restart; a failure after the first 5,000-event page can be
  retried to consistent event and bucket costs without an early metadata switch.
- Many-bucket reconciliation includes at least one orphan and structurally
  proves the final reconciliation has no `usage_event` dependency.
- Embedded-upgrade progress tests cover ordering, monotonic counts, successful
  activation, no-op silence, failed-run completion suppression, and retry.
- Context pressure for embedded and configured models; unknown windows remain
  counted separately.
- Final gates: fmt, strict Clippy, serial full tests, docs build, and diff check.
