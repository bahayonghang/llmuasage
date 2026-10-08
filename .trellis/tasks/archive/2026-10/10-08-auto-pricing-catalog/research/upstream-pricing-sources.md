# Upstream pricing sources

Checked 2026-10-08 in `ref/repo/ccusage` and `ref/repo/tokscale`.

## Shared table

Both projects use LiteLLM's public price table:

`https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json`

- ccusage: `ref/repo/ccusage/rust/crates/ccusage-core/src/pricing.rs` (`LITELLM_PRICING_URL`, `fetch_pricing_json`).
- tokscale: `ref/repo/tokscale/crates/tokscale-core/src/pricing/litellm.rs` (`PRICING_URL`).

## Second table

Both also use models.dev when LiteLLM does not publish the model or its long-context rates:

`https://models.dev/api.json`

- ccusage loads it for models missing from LiteLLM and calls `fill_long_context_rates_from_models_dev`.
- tokscale fetches it beside LiteLLM in `PricingService::fetch_inner`.

## Tokscale-only table

tokscale also calls OpenRouter at `https://openrouter.ai/api/v1`. ccusage does not. This task does not add that third request to default sync.

## Failure behavior

ccusage `PricingMap::load_with_overrides`:

- Default command load fetches LiteLLM once.
- `--offline` skips the network.
- Fetch or parse failure keeps the build-time embedded snapshot. There is no user disk cache of the last successful download.

tokscale:

- Disk cache TTL is 3600 seconds (`pricing/cache.rs`). A fresh cache returns before any network call.
- Fetch failure uses that source's cached file at any age (`load_cached_any_age`).
- A source with no cache contributes nothing. The other sources still price what they cover.
- Successful fetches are saved with a temp-file rename so a crash cannot delete the previous cache first.

## Decision for llmusage

Follow tokscale's one-hour skip, as chosen on 2026-10-08:

- Each source is timed separately. A successful cache no older than 3600 seconds is used without a request.
- A missing cache, a cache older than 3600 seconds, or a future timestamp is fetched once.
- Failure or an unusable document keeps that source's previous successful cache at any age.
- No cache and a failed refresh falls back to the embedded catalog. That is the cold start, not the ongoing price source.
- OpenRouter stays out of default sync.

The current LiteLLM importer in `src/domain/pricing_catalog.rs` drops tiers. A dynamic import has to preserve published long-context thresholds, which both reference projects already model (`above_200k`, `above_272k`, and neighboring bands).
