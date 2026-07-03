# Implementation Plan - 添加 Claude Fable/Mythos 模型统计

## Gate 0 - Before Editing

- [x] Re-check official Claude docs if implementation happens after 2026-07-03, because model
      availability/pricing is time-sensitive.
- [x] Confirm the working tree is clean or identify unrelated changes before editing.
- [x] Keep this task in planning until artifacts are approved; do not run `task.py start` until
      review gate passes.

## Step 1 - Static Catalog Rows

- [x] Edit `pricing/static-v1.json`.
- [x] Add Claude Fable 5 and Mythos 5 row(s) for `source = "claude"`.
- [x] Add OpenCode/Anthropic row(s) only with explicit matchers, not a broad "all Anthropic" matcher.
- [x] Include `cache_creation_per_mtok = 12.5` and `context_window = 1000000`.

Validation:

- [x] `cargo test pricing_catalog_loads_static_v1`

Rollback:

- Remove the new Fable rows.

## Step 2 - Catalog And Cost Tests

- [x] Update `src/query/pricing_catalog.rs` tests for Fable matcher coverage and context window.
- [x] Add equivalent Mythos matcher/context-window assertions.
- [x] Update `src/query/pricing.rs` tests for Fable/Mythos static cost calculation.
- [x] Ensure existing matcher tests still reject accidental broad matches such as `not-fable`,
      `not-mythos`, and `mythos-preview` unless explicitly scoped later.

Suggested cost fixture:

- input `1_000_000`
- cache read `200_000`
- cache creation `300_000`
- output `400_000`
- expected cost with cache: `33.95` (`10 + 0.2 + 3.75 + 20`)
- expected without-cache cost: `35.0`

Validation:

- [x] `cargo test pricing_catalog`
- [x] `cargo test pricing`

Rollback:

- Revert test additions with catalog row removal.

## Step 3 - Context Pressure Fixture

- [x] Add or extend a `Dashboard::context_pressure` test with `source = "claude"` and
      `model = "claude-fable-5"` / `model = "claude-mythos-5"`.
- [x] Assert 500k prompt tokens gives `peak_percent = 0.5` against a 1M context window.
- [x] Assert Fable/Mythos rows are not counted in `unpriced_events`.

Validation:

- [x] `cargo test context_pressure`

Rollback:

- Remove the Fable fixture.

## Step 4 - Parser/Report Integration Fixture

- [x] Seed Claude JSONL fixtures using `message.model = "claude-fable-5"` and
      `message.model = "claude-mythos-5"`.
- [x] Assert sync/report/model breakdown preserves the Fable/Mythos model names and produces non-zero
      static cost.
- [x] Assert the report does not show `unpriced` for those fixtures.

Likely surfaces:

- `tests/local_flow.rs` for end-to-end local source import.
- `tests/report_commands.rs` if a lighter report-only seeded fixture is enough.

Validation:

- [x] Targeted test for the chosen fixture file.

Rollback:

- Remove the fixture and assertions.

## Step 5 - Documentation/Release Check

- [x] Search docs for static model coverage claims.
- [x] If changed behavior needs a user-facing note, update the matching English and Chinese docs.
- [x] Explicitly record user action: new imports/rebuilds pick up Fable/Mythos pricing; existing already
      imported rows may require `llmusage sync --rebuild` if users need historical repricing.

Validation:

- [x] `npm --prefix docs run docs:build` only if docs changed.

## Final Gate

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test -- --test-threads=1`
- [x] `git diff --check`
- [x] Confirm task notes still state Mythos is included.

## Implementation Notes

- `cache_creation_per_mtok = 12.5` intentionally uses the official 5-minute cache write rate as the current schema's single aggregate cache-write approximation. Exact 1-hour cache-write billing still needs a separate token-schema task.
- No user-facing docs list embedded model coverage, so no README/VitePress docs change was needed.
- New imports or rebuilds pick up Fable/Mythos static pricing; already imported rows may need `llmusage sync --rebuild` if historical repricing is required.
