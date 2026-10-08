# Current pricing update path

Recorded 2026-10-08. This is why a per-model catalog edit is the wrong task.

## What exists

- Embedded base: `pricing/static-v2.json`. `schema_version` is `2`. Document `version` is `static-v3`.
- Unpinned `static-*` databases reprice only when `active.starts_with("static-") && active != embedded.version` in `src/store/pricing_catalog.rs`.
- `llmusage catalog apply <file>` merges a local overlay. `doctor --refresh-pricing <file>` activates one complete local base snapshot.
- `PricingCatalog::load_snapshot` accepts catalog v2 or a native LiteLLM `model_prices_and_context_window.json`. It refuses URLs.
- README states that llmusage does not fetch pricing from the network.

## LiteLLM import limits

`native_litellm_model` in `src/domain/pricing_catalog.rs`:

- Maps Anthropic / `claude` rows to sources `claude` and `opencode`.
- Maps OpenAI / `gpt` rows to sources `codex` and `opencode`.
- Builds family matchers from the model id.
- Sets `tiers` to an empty list. A LiteLLM file that only has flat token rates therefore cannot express the 272K long-context tier by itself through this importer.

## Why new ids are wrong today

The embedded catalog has no `claude-sonnet-5-5`, `claude-haiku-5-5`, or `gpt-6.1-sol` row.

- OpenCode family `gpt` matches any normalized id that starts with `gpt-`.
- Claude families `claude-sonnet`, `claude-haiku`, and `claude-opus` match newer ids with the same prefix.
- Codex family `gpt-5` does not match `gpt-6.1-sol`, so Codex Sol 6.1 stays unpriced unless another row claims it.

## Existing self-update

`llmusage update` installs a newer binary from the official Git repository. It does not refresh the pricing catalog on its own. A newly shipped embedded catalog only becomes active after that binary runs and an unpinned `static-*` database is upgraded.

## Version

Crate, desktop crate, `desktop/src-tauri/tauri.conf.json`, `desktop/package.json`, and both READMEs say `2.0.0`. `CHANGELOG.md` has `## 2.0.0 - Unreleased`. `git tag -l "v2*"` printed no tags.

## Named examples, not rows to paste

Vendor pages checked before the scope correction:

- `claude-sonnet-5-5`: input `$2`, output `$10`, 5m cache write `$2.50`, 1h cache write `$4`, cache read `$0.10`, context `1_000_000`. Source: https://platform.claude.com/docs/en/models/sonnet-5-5/overview
- `gpt-6.1-sol`: input `$2`, cached input `$0.10`, cache write `$2.50`, output `$10`, context `1_050_000`. Prompts above `272_000` tokens use 2x input and cache rates and 1.5x output for the whole request. Published snapshot is `gpt-6.1-sol` only. Source: https://developers.openai.com/api/docs/models/gpt-6.1-sol

`claude-haiku-5-5` was named by the user. Its full price card was not saved in this research pass. The Sonnet 5.5 overview only says Haiku 5.5 starts at `$0.10 / $0.50` per MTok.
