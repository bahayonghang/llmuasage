# Current catalog gaps

Evidence from `pricing/static-v2.json` and `src/domain/pricing_catalog.rs` on 2026-09-05.

## Matcher rule

`matcher_matches` (`src/domain/pricing_catalog.rs:725-737`):

- `exact`: normalized equality
- `family`: equality or `normalized_model.starts_with("{matcher}-")`

Normalization lowercases, strips a `/` provider prefix, maps `.` `_` `:` to `-`.

`PricingCatalog::find` (`src/domain/pricing_catalog.rs:370-393`) picks exact over family, then longest matcher. Adding a more specific row is enough; matcher engine changes are not required.

## GPT-6 Astra today

Codex `gpt-5-legacy-codex` family is `gpt-5` / `o3` / `o4`. `gpt-6-astra` does not start with `gpt-5-`, so Codex Astra events are **unpriced**.

OpenCode `gpt-5-legacy-opencode` includes family `gpt` (`pricing/static-v2.json:129`). `gpt-6-astra` starts with `gpt-`, so OpenCode Astra events are **mispriced** at GPT-5 rates (`1.25 / 0.125 / 10.0`), not unpriced.

GPT-5.6 avoided this by shipping **exact** ids plus the exact `gpt-5.6` → Sol alias, which outrank the GPT-5 family. Astra needs the same treatment.

`gpt-6-rewrite` in `tests/sync/sources/pi_omp.rs` is an unrelated Pi fixture id. Do not attach Astra rates to it.

## Claude Fable 5.1 today

Existing row `claude-fable-5` uses family matchers `claude-fable-5`, `fable-5`, `anthropic-claude-fable-5` (`pricing/static-v2.json:89-104`).

`claude-fable-5-1` starts with `claude-fable-5-`, so Fable 5.1 is **mispriced** as Fable 5: cache read `1.00` instead of `0.25`. Input/output happen to match; cache-heavy agent sessions would be overcharged in reports.

The same prefix trap applies to `claude-mythos-5` → `claude-mythos-5-1`.

A Fable 5.1 family matcher `claude-fable-5-1` is longer than `claude-fable-5`, so `find` will select 5.1 without changing Fable 5 rows.

## Version bump

Bootstrap reprices only when `active.starts_with("static-") && active != embedded.version` (`src/store/pricing_catalog.rs:496-500`). Mutating `static-v2.json` while keeping `version: "static-v2"` leaves already-imported mispriced Astra / Fable 5.1 events on the old rates.

The catalog `version` label must change (recommend `static-v3`). `schema_version` stays `2`. Filename may stay `pricing/static-v2.json` (schema file) or move; that is a design choice, not a product choice.

Pinned snapshots and overlays stay pinned (`rebase_available`). This task does not force overlay rebase.

## Adjacent, not this task

OpenAI's live GPT-5.6 Standard list is now lower than the embedded GPT-5.6 rows (Sol catalog `5.0/0.5/30.0` vs current docs `4.0/0.4/20.0` promotional). Do not silently reprice GPT-5.6 in this task.

Anthropic 1h cache-write `20.0` still cannot be expressed: parsers aggregate cache creation. Keep the 5m write rate `12.5` as the single `cache_creation_per_mtok`, same as Fable 5.

Fast / Batch / Flex / data-residency uplifts are not in local JSONL today. Do not add service-tier columns.
