# llmusage Pricing Catalog Contracts

## Scenario: Static Model Pricing And Context Windows

### 1. Scope / Trigger

- Trigger: changes to `pricing/static-v1.json`, `PricingCatalog`, cost
  calculation, model breakdown pricing fields, or context-pressure windows.
- Static pricing is cross-layer: parser model strings flow into SQLite events,
  `PricingCatalog::find`, persisted cost fields, dashboard/TUI model breakdowns,
  and context pressure.
- Parser-owned model names must remain raw model identifiers. Catalog changes
  add recognition; they do not rename stored events into another model bucket.

### 2. Signatures

- Static catalog asset: `pricing/static-v1.json`.
- Lookup API: `PricingCatalog::static_v1().find(source, model) ->
  Option<&PricingRow>`.
- Context API: `PricingCatalog::context_window(source, model) -> Option<i64>`.
- Cost API: `compute_cost(source, model, UsageTokens) -> CostEstimate`.
- Relevant persisted fields: `cost_with_cache_usd`,
  `cost_without_cache_usd`, `pricing_status`, `pricing_source`,
  `pricing_rate`.

### 3. Contracts

- A model is priced only when the catalog has a row for that source and the
  matcher is intentionally scoped to the model family/version.
- `pricing_status = "static"` and `pricing_source = "static-v1"` identify
  embedded catalog hits; misses stay `pricing_status = "unpriced"`.
- `context_pressure` only uses rows with a known context window. Unknown models
  must increase the unpriced/unknown-window count rather than guessing a
  denominator.
- Provider-prefixed OpenCode/Anthropic model IDs should be covered with
  explicit matchers when normalization cannot strip the prefix safely.
- `UsageTokens.cache_creation_tokens` is one aggregate field. If official
  pricing distinguishes cache write durations, the static row may only use one
  documented approximation until a schema-level split exists.

### 4. Validation & Error Matrix

- Missing static row -> cost is zero, `pricing_status = "unpriced"`, and no
  context denominator is available.
- Over-broad matcher -> unrelated models can be repriced; add negative tests
  such as `not-<model>` and preview/non-version aliases.
- New model context window omitted -> costs may be priced while context
  pressure still treats the model as unknown.
- Exact cache-write duration unavailable in stored tokens -> document the
  chosen single-rate approximation in task notes or code comments.

### 5. Good/Base/Bad Cases

- Good: `claude-fable-5` and `claude-mythos-5` keep their stored model names
  and both resolve to explicit Claude static rows with a 1,000,000-token window.
- Base: existing Opus/Sonnet/Haiku/GPT/Gemini rows keep passing non-overlap
  tests after adding a model row.
- Bad: using a broad matcher like `mythos` when the task only covers
  `claude-mythos-5`, causing `claude-mythos-preview` to match accidentally.

### 6. Tests Required

- Static catalog tests for positive source/model hits and negative non-overlap
  cases.
- Cost tests asserting `PricingStatus::Static`, source `static-v1`,
  non-zero cost, and expected rates in `pricing_rate`.
- Context-pressure tests asserting known windows are used and unknown models
  remain counted as unpriced/unknown.
- Parser/sync/report fixture when a new source model should appear in user
  reporting without becoming `unpriced`.

### 7. Wrong vs Correct

#### Wrong

```json
{
  "source": "claude",
  "matchers": ["mythos"],
  "input_per_mtok": 10.0
}
```

#### Correct

```json
{
  "source": "claude",
  "matchers": ["claude-mythos-5", "mythos-5"],
  "input_per_mtok": 10.0,
  "cached_per_mtok": 1.0,
  "cache_creation_per_mtok": 12.5,
  "output_per_mtok": 50.0,
  "context_window": 1000000
}
```
