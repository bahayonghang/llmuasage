# Implementation Plan — GPT-6 Astra 与 Claude Fable/Mythos 5.1 定价覆盖

## Gate 0 — Before Editing

- [ ] Re-check OpenAI Astra and Anthropic Fable 5.1 / Mythos 5.1 pages if implementation starts after 2026-09-05.
- [ ] Confirm the working tree is clean or list unrelated dirty files and leave them untouched.
- [ ] Do not run `task.py start` until the user approves this planning summary.

## Step 1 — Catalog Data And Identity

Files:

- `pricing/static-v2.json`
- `src/domain/pricing_catalog.rs` (`embedded()` fallback string and comments that claim the identity is `static-v2`)

Actions:

- Set `"version": "static-v3"`. Keep `"schema_version": 2` and the filename.
- Insert `gpt-6-astra` next to the GPT-5.6 block with exact + family matchers and the 272K tier from `design.md`.
- Insert `claude-fable-5-1` and `claude-mythos-5-1` next to the 5.0 rows with family matchers from `design.md`.
- Leave Fable 5 / Mythos 5 / GPT-5 / GPT-5.6 rows unchanged.
- Update `catalog_document_from_str(..., Some("static-v3"))`.

Validation:

```
cargo test --lib pricing_catalog_loads -- --test-threads=1
```

Rollback: restore `pricing/static-v2.json` and the fallback string.

## Step 2 — Unit Tests For Find, Cost, Window

Files:

- `src/domain/pricing_catalog.rs` tests
- `src/domain/pricing.rs` tests
- `src/query/tests/overview_breakdowns.rs`

Actions:

- Assert embedded identity `static-v3`.
- Positive: Codex/OpenCode `gpt-6-astra`; Claude/OpenCode Fable 5.1 and Mythos 5.1 including dotted and `anthropic.` / `anthropic/` forms.
- Negative: `not-gpt-6-astra`, `gpt-6-rewrite`, `not-fable-5-1`, `not-mythos-5-1`, `claude-mythos-preview`.
- Regression: Fable 5 / Mythos 5 still `cached_per_mtok = 1.0` and cost `33.95`.
- Astra short `7.0` / long `11.500025`; Fable/Mythos 5.1 cost `33.80` / `35.0`.
- Context pressure: Fable 5.1 or Mythos 5.1 uses 1M; Astra uses 1.05M; unknown preview still unpriced.

Validation:

```
cargo test --lib pricing_catalog -- --test-threads=1
cargo test --lib pricing_static -- --test-threads=1
cargo test --lib context_pressure -- --test-threads=1
```

Rollback: revert the test additions with the catalog rows.

## Step 3 — Sync Fixtures

Files:

- `tests/cli/local_flow.rs`

Actions:

- Extend or add Claude seed rows for `claude-fable-5-1` and `claude-mythos-5-1` with the same token shape as the 5.0 seed.
- Add Codex + OpenCode seeds for `gpt-6-astra` short and long, mirroring `seed_codex_gpt_5_6` / `seed_opencode_gpt_5_6`.
- Assert stored `model` strings stay raw, `pricing_status = static`, `pricing_source = static-v3`, and the costs from Step 2.

Validation:

```
cargo test --test local_flow sync_prices -- --test-threads=1
```

If the test binary name differs, run the `sync_prices_claude_fable` / `sync_prices_gpt_5_6` names plus the new test.

Rollback: remove the new seeds and assertions.

## Step 4 — Live-Identity String Sweep

Files that currently assert the live embedded identity `"static-v2"` (not historical v1→v2 progress copy):

- `src/domain/pricing.rs`
- `src/domain/pricing_catalog.rs`
- `tests/cli/local_flow.rs`
- `tests/cli/operations.rs`
- `src/query/tests/pricing.rs`
- `src/query/tests/overview_breakdowns.rs`
- `src/store/pricing_catalog.rs`
- `src/store/sync_writer.rs`

Actions:

- Change live-identity expectations to `static-v3`, or to `PricingCatalog::embedded().version` where that is less brittle.
- Leave `src/commands/sync_progress.rs` and `src/store/mod.rs` `pricing_slow_warning_threshold_is_one_shot` on the historical `static-v1 → static-v2` copy.

Validation:

```
rg "static-v2" --glob "*.rs"
```

Every remaining hit must be a historical progress fixture, a filename `static-v2.json`, or a comment about the schema file.

## Step 5 — Spec, README, Architecture, Changelog

Files:

- `.trellis/spec/llmusage/backend/pricing-catalog-contracts.md`
- `README.md`
- `README.zh-CN.md`
- `docs/architecture/index.md`
- `docs/zh/architecture/index.md`
- `CHANGELOG.md`

Actions:

- Spec §2: file `pricing/static-v2.json`, current identity `static-v3`.
- Spec new §9: Astra exact+family and 272K tier; Fable/Mythos 5.1 family matchers, cache read `0.25`, 1M window, 5m write approximation.
- README / architecture: list Astra, Fable 5.1, Mythos 5.1 and the `static-v3` identity.
- Changelog Unreleased: catalog coverage + bootstrap reprice of unpinned `static-v2` databases.

Validation:

```
npm --prefix docs run docs:build
```

## Final Gate

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test -- --test-threads=1
git diff --check
```

If docs changed, keep the docs build from Step 5.

## Risky Files

- `pricing/static-v2.json` — matcher overlap with `gpt` and `claude-fable-5` / `claude-mythos-5`.
- `src/store/pricing_catalog.rs` — overlay tests pin `static-v2` as the embedded identity.
- `tests/cli/local_flow.rs` — OpenCode sqlite seed helpers assume a single `opencode.db`; new Astra seed must not collide with existing OpenCode seeds in the same test.

## Follow-Up Before `task.py start`

- User has approved this planning summary in a later message.
- `implement.jsonl` and `check.jsonl` already have real spec/research entries.
- No product-code edits in the planning turn.
