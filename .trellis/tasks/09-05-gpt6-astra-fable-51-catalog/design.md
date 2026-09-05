# Design — GPT-6 Astra 与 Claude Fable/Mythos 5.1 定价覆盖

## Classification

Complex catalog change. Touches embedded pricing data, catalog identity, cost tests, context-pressure fixtures, sync fixtures, README/architecture docs, and the pricing-catalog spec. Does not change parsers, `SourceKind`, or SQLite schema.

## Current Failure Mode

```
parser JSONL/DB  --raw model string--> usage_event.model
                                            |
                                            v
                         PricingCatalog::find(source, model)
                                            |
                 Codex Astra: no gpt-5- prefix -> unpriced
                 OpenCode Astra: gpt family     -> GPT-5 rates
                 Fable/Mythos 5.1: *-5- prefix  -> 5.0 cache read 1.00
```

`find` already prefers exact over family, then longest matcher. New rows with longer / exact matchers are sufficient. Matcher engine stays unchanged.

## Catalog Identity

Keep file `pricing/static-v2.json` because `schema_version` stays `2`.

Change the document `version` field to `static-v3`.

`PricingCatalog::embedded()` currently does `catalog_document_from_str(STATIC_V2_JSON, Some("static-v2"))`. The JSON `version` field is authoritative for v2 documents; still update the fallback string to `static-v3` so a missing field cannot stamp the old identity.

Bootstrap (`src/store/pricing_catalog.rs:496-500`) reprices when `active.starts_with("static-") && active != embedded.version`. Unpinned `static-v2` databases reprice on the next `sync`. Pinned snapshots and overlays stay pinned and report `rebase_available` if their base identity is old.

Do not rename the JSON file. Do not introduce `pricing/static-v3.json`. That would duplicate the schema-2 document and expand the include path churn.

## Model Rows

Add three definitions to `pricing/static-v2.json`. Place Astra next to the GPT-5.6 block. Place Fable/Mythos 5.1 next to the existing Fable/Mythos 5 rows.

### `gpt-6-astra`

```json
{
  "id": "gpt-6-astra",
  "sources": ["codex", "opencode"],
  "matches": [
    { "value": "gpt-6-astra", "mode": "exact" },
    { "value": "gpt-6-astra", "mode": "family" }
  ],
  "rates": {
    "default": {
      "input_per_mtok": 10.0,
      "cached_per_mtok": 1.0,
      "cache_creation_per_mtok": 12.5,
      "output_per_mtok": 50.0
    },
    "tiers": [
      {
        "name": "long_context",
        "prompt_tokens_above": 272000,
        "input_per_mtok": 20.0,
        "cached_per_mtok": 2.0,
        "cache_creation_per_mtok": 25.0,
        "output_per_mtok": 75.0
      }
    ]
  },
  "context_window": 1050000
}
```

Exact outranks OpenCode `gpt` family. Family covers dated ids such as `gpt-6-astra-2026-09-03`. Duplicate `(source, mode, value)` is rejected; exact and family are different modes, so both values are legal.

Do not add `gpt-6`. A later `gpt-6-mini` at a different rate would be claimed.

Do not add `gpt-6-astra-aeon`. No public rate card.

### `claude-fable-5-1` and `claude-mythos-5-1`

```json
{
  "id": "claude-fable-5-1",
  "sources": ["claude", "opencode"],
  "matches": [
    { "value": "claude-fable-5-1", "mode": "family" },
    { "value": "fable-5-1", "mode": "family" },
    { "value": "anthropic-claude-fable-5-1", "mode": "family" }
  ],
  "rates": {
    "default": {
      "input_per_mtok": 10.0,
      "cached_per_mtok": 0.25,
      "cache_creation_per_mtok": 12.5,
      "output_per_mtok": 50.0
    }
  },
  "context_window": 1000000
}
```

Mythos 5.1 uses the same rates and window, matchers `claude-mythos-5-1`, `mythos-5-1`, `anthropic-claude-mythos-5-1`.

Length wins over the existing 5.0 family matchers:

| Candidate | Winning matcher |
| --- | --- |
| `claude-fable-5` | `claude-fable-5` family |
| `claude-fable-5-1` | `claude-fable-5-1` family |
| `claude-fable-5.1` | normalize to `claude-fable-5-1` |
| `anthropic.claude-fable-5-1` | `anthropic-claude-fable-5-1` family |
| `anthropic/claude-fable-5-1` | strip `/`, then `claude-fable-5-1` family |

Do not add matcher `mythos`. It would claim `mythos-preview`.

Keep Fable 5 / Mythos 5 rows unchanged so remaining 5.0 logs stay at cache read `1.00`.

## Cost Numbers

`cost_without_cache_usd` prices all prompt tokens at the input rate. `src/domain/pricing.rs:122-125`

Fable/Mythos 5.1 with `(input, cache_read, cache_creation, output) = (1e6, 2e5, 3e5, 4e5)`:

- with cache: `10 + 0.05 + 3.75 + 20 = 33.80`
- without cache: `1.5 * 10 + 20 = 35.0`

Astra with GPT-5.6's short/long token pair `(1e5, 1e5, 72_000|72_001, 1e5)`:

- default (`prompt_tokens = 272000`, not above threshold): `1 + 0.1 + 0.9 + 5 = 7.0`
- long_context: `2 + 0.2 + 1.800025 + 7.5 = 11.500025`

Reasoning policy remains `included_in_output`. Public Astra and Fable/Mythos 5.1 tables do not define a separate reasoning channel.

`cache_creation_per_mtok = 12.5` remains the 5m write approximation. 1h write `20.0` still cannot be expressed.

## Compatibility

- No schema migration.
- No new `SourceKind`.
- No production Rust alias map.
- Existing GPT-5, GPT-5.6, Fable 5, Mythos 5, Opus/Sonnet/Haiku, Gemini rows keep current rates.
- Overlay users stay on their recorded base until they apply again or `catalog reset`.
- `gpt-6-rewrite` in Pi/OMP tests stays unrelated.

## Spec And Docs

Update `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`:

- §2: embedded file remains `pricing/static-v2.json`; current identity is `static-v3`.
- New §9: Astra exact+family, 272K tier, Fable/Mythos 5.1 family matchers and cache read `0.25`.

User-facing copy in `README.md`, `README.zh-CN.md`, `docs/architecture/index.md`, `docs/zh/architecture/index.md`: mention `static-v3` identity and the three new models. `CHANGELOG.md` Unreleased records the catalog addition and the bootstrap reprice.

Prefer `PricingCatalog::embedded().version` in new assertions so the next identity bump does not require another string sweep. Existing live-identity assertions that hardcode `"static-v2"` must move to `"static-v3"`. Synthetic progress-copy fixtures that narrate a historical `static-v1 → static-v2` upgrade stay unchanged (`src/commands/sync_progress.rs`, `src/store/mod.rs:1331-1340`).

## Rollback

Revert the three catalog rows, set `version` back to `static-v2`, and revert tests/docs/spec. Databases remain readable. Unpinned users who already upgraded to `static-v3` would reprice again on the next binary that still ships `static-v2`, restoring the previous mispricing for Astra / 5.1 events.
