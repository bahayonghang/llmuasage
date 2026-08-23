# ADR 0016 — Source-reported cost for Pi / Oh My Pi

- Status: Accepted
- Date: 2026-08-23
- Related code: `src/query/pricing.rs`, `src/store/sync_writer.rs`, `src/store/mod.rs`, `src/parsers/pi.rs`, `src/domain/models.rs`
- Related terms: Source, Usage Event, Pricing Status, Pricing Catalog, Source Cost

## Context

Pi and Oh My Pi usage records carry `usage.cost{input,output,cacheRead,cacheWrite,total}`
in USD. The embedded catalog has no `pi`/`omp` rows, so those events previously
stayed `unpriced` even when the source reported a positive total.

The catalog lookup is `(source, model)`. Routed names such as `stealth/ox-alpha`
and `deepseek-v4-flash` cannot be covered by a finite static table. A user
overlay may still add `omp` rows.

`Store::recompute_costs` pages every event, writes catalog costs, then
reconciles `usage_bucket_30m` from that in-memory rollup. Skipping a
`source_reported` event without folding its persisted cost into the rollup
would zero the bucket or delete it as an orphan.

Older binaries decode unknown `pricing_status` values as `unpriced`.

## Decision

Add `PricingStatus::SourceReported` (`as_str() == "source_reported"`).

Cost selection at shard write:

1. `event.source_cost` exists and `total > 0` → persist the source-reported
   USD and stamp `source_reported`.
2. Otherwise use `compute_cost_with` against the active catalog.

`total == 0`, a missing `cost`, and a non-object `cost` all take the catalog
path. They are not treated as free. With the embedded catalog the result is
`unpriced`. With a user overlay or snapshot that contains an `omp` row, the
catalog status (`static` / `snapshot`) is kept.

`cost_without_cache_usd` derivation:

- When `input_tokens > 0` and `cost.input > 0`:
  `input_rate = cost.input / input_tokens`,
  `without_cache = (input + cache_read + cache_creation) * input_rate + cost.output`.
- Otherwise `without_cache = cost.total` and `pricing_rate` records
  `{"source":"pi_usage_cost","without_cache":"fallback_equals_total"}`.

`pricing_source` is `"source-reported"`. Reasoning is not part of the
derivation; it is already inside output.

Recompute reads persisted cost columns. For `source_reported` rows it skips
`UPDATE` and still folds the persisted breakdown into the bucket rollup.

Path-reset bucket decoding recognizes `"source_reported"`. Unknown values
remain `unpriced`.

`UsageEvent.source_cost` is `#[serde(default)]`. Old shard JSON without the
field deserializes as `None`. Schema version does not increase. Token
accounting versions do not increase. Historical backfill is
`sync --rebuild --source omp`.

## Rejected Alternatives

- Add `pi`/`omp` rows to `pricing/static-v2.json`. Rejected because routed
  model names are not a closed set.
- Treat `total == 0` as free. Rejected because xai-oauth and openrouter
  records are always zero and cannot be distinguished from unreported cost.
- Skip `source_reported` rows in recompute without folding them into buckets.
  Rejected because stage-2 orphan deletion would drop those buckets.
- Persist raw cost components as extra SQLite columns. Rejected because
  `pricing_status='source_reported'` is enough to protect the row, and a
  formula change can replay the source.

## Consequences

- Positive Pi / Oh My Pi `usage.cost.total` lands as non-zero event and bucket
  cost with status `source_reported`.
- Catalog recompute and overlay apply cannot zero those amounts.
- `unpriced` report and dashboard notes stay tied to `pricing_status ==
  unpriced`. `source_reported` counts as priced.
- Older binaries that read a database written by this version decode
  `source_reported` as `unpriced`. The numeric cost columns remain readable.
- Rollback is code revert plus `sync --rebuild --source omp`. Costs return to
  `unpriced` under the embedded catalog.

## Verification

- `PricingStatus::SourceReported.as_str()` is `source_reported`.
- Writer tests cover `total > 0`, `total == 0`, missing cost, non-object cost,
  and an overlay `omp` row that prices `total == 0` as `snapshot`.
- Recompute tests keep event amounts and both pure and mixed buckets.
- Path reset keeps bucket status `source_reported`.
- Old shard JSON without `source_cost` deserializes.
- `unpriced` report notes do not include `source_reported` rows.
