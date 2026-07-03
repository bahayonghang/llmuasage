# Design - 添加 Claude Fable/Mythos 模型统计

## Classification

Complex backend/catalog change. It touches pricing metadata, cost computation tests, context-window
statistics, and parser/report fixtures, but it should not change source discovery, parser ownership,
or database schema.

## Root Cause

`llmusage` already stores Claude model names as raw normalized strings. Claude JSONL rows with
`message.model = "claude-fable-5"` or `message.model = "claude-mythos-5"` should become
matching `usage_event.model` rows.

The gap is downstream catalog coverage:

```
Claude JSONL -> parsers/claude.rs -> usage_event.model
                                     |
                                     v
pricing/static-v1.json -> PricingCatalog::static_v1()
                                     |
                  cost columns + model breakdown + context_pressure
```

Because `pricing/static-v1.json` has no Fable/Mythos matcher, those rows currently behave like
unknown models: zero static cost, `pricing_status = "unpriced"`, and no context-window
denominator.

## Static Catalog Contract

Add Fable/Mythos rows to `pricing/static-v1.json`:

- `source = "claude"` for Claude's local JSONL source.
- `source = "opencode"` for OpenCode/provider-prefixed Anthropic model IDs if the matcher can be
  expressed without broadening source semantics.

Recommended matchers:

- Claude source: `["claude-fable-5", "fable-5", "claude-mythos-5", "mythos-5"]`
- OpenCode source:
  `["claude-fable-5", "fable-5", "anthropic-claude-fable-5", "claude-mythos-5", "mythos-5", "anthropic-claude-mythos-5"]`

`PricingCatalog` normalizes `/`, `.`, `_`, and `:` in model candidates, but it only strips provider
prefixes separated by `/`. The explicit `anthropic-claude-fable-5` matcher covers dot-style Bedrock
IDs such as `anthropic.claude-fable-5` after normalization; the Mythos matcher does the same for
`anthropic.claude-mythos-5`.

Do not use broad `mythos` as a matcher. It would also match `mythos-preview` through the existing
dash-prefix logic, which is broader than this task's Claude Mythos 5 scope.

## Pricing Values

Use official Fable 5 / Mythos 5 public rates:

- input: `10.0`
- cached reads / refreshes: `1.0`
- cache creation: `12.5`
- output: `50.0`
- context window: `1_000_000`

`cache_creation_per_mtok = 12.5` is a conscious MVP approximation. Official pricing differentiates
5m cache writes (`12.5`) and 1h cache writes (`20.0`), but current storage aggregates both into
`cache_creation_tokens`. The task should not introduce schema work just to split those rates.

Reasoning tokens remain `ReasoningPolicy::IncludedInOutput`, matching the current catalog default:
Fable/Mythos have always-on adaptive thinking, but the public pricing table does not define a
separate reasoning-token charge.

## Compatibility

- No schema migration.
- No new `SourceKind`.
- No parser behavior change unless a fixture exposes that model extraction fails, which current
  code inspection does not indicate.
- Existing Opus/Sonnet/Haiku rows should keep their current rates and match behavior.
- Existing imported Fable/Mythos rows are not automatically repriced on startup. New imports/rebuilds
  use the embedded static catalog; historical repricing remains an explicit rebuild/recompute concern.

## Tests

1. `src/query/pricing_catalog.rs`
   - Static catalog loads and finds Fable/Mythos for `claude`.
   - Static catalog finds Fable/Mythos for OpenCode provider-prefixed forms selected above.
   - Fable/Mythos context window is `Some(1_000_000)`.
   - Non-overlap validation still passes.
2. `src/query/pricing.rs`
   - `compute_cost("claude", "claude-fable-5", tokens)` yields static status and expected cost.
   - `compute_cost("claude", "claude-mythos-5", tokens)` yields the same static status/rates.
   - `pricing_rate` includes the Fable/Mythos rates, including `cache_creation_per_mtok = 12.5`.
3. `src/query/mod.rs`
   - `context_pressure` fixture with Fable/Mythos uses the 1M denominator and does not count it as
     unknown/unpriced context.
4. Parser/sync/report fixture
   - Seed Claude Fable/Mythos JSONL events and assert model/cost reporting includes the model names
     with non-zero static cost and no `unpriced` marker.

## Rollback

Rollback is limited to removing the Fable/Mythos rows and tests. Because no migration is introduced,
databases remain compatible; only future Fable/Mythos imports return to unpriced/unknown-context
behavior.
